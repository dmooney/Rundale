// Analyze a simulator recording of a streamed reply, frame by frame.
//
//   swift mobile/scripts/stream-frames.swift <video> <output-dir> \
//       --speaker "Peig Hannigan" --busy "Having a think"
//
// simctl records a frame only when the screen changes, so presentation
// timestamps give the rendering cadence directly. Every changed frame is read
// with Vision text recognition. A frame is "provisional" when the speaker's
// row is on screen while the busy indicator is still shown, and "final" once
// the speaker's row remains and the indicator is gone. The tool writes
// `stream-frames.json` and PNGs of each distinct provisional text state and the
// first final frame, then exits nonzero unless provisional text rendered
// before the final frame.

import AVFoundation
import CoreImage
import Foundation
import ImageIO
import Vision

struct Options {
    var video = ""
    var output = ""
    var speaker = ""
    var busy = ""
}

func parseOptions() -> Options {
    var options = Options()
    var positional: [String] = []
    var arguments = CommandLine.arguments.dropFirst().makeIterator()
    while let argument = arguments.next() {
        switch argument {
        case "--speaker": options.speaker = arguments.next() ?? ""
        case "--busy": options.busy = arguments.next() ?? ""
        default: positional.append(argument)
        }
    }
    guard positional.count == 2, !options.speaker.isEmpty, !options.busy.isEmpty else {
        FileHandle.standardError.write(Data(
            "usage: stream-frames.swift <video> <output-dir> --speaker NAME --busy TEXT\n".utf8
        ))
        exit(2)
    }
    options.video = positional[0]
    options.output = positional[1]
    return options
}

struct Frame {
    let index: Int
    let time: Double
    let lines: [String]
    let image: CIImage
}

func recognizedLines(_ image: CIImage) throws -> [String] {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    try VNImageRequestHandler(ciImage: image).perform([request])
    return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
}

func signature(_ pixels: CVPixelBuffer) -> [UInt8] {
    CVPixelBufferLockBaseAddress(pixels, .readOnly)
    defer { CVPixelBufferUnlockBaseAddress(pixels, .readOnly) }
    let width = CVPixelBufferGetWidth(pixels)
    let height = CVPixelBufferGetHeight(pixels)
    let rowBytes = CVPixelBufferGetBytesPerRow(pixels)
    let base = CVPixelBufferGetBaseAddress(pixels)!.assumingMemoryBound(to: UInt8.self)
    var values: [UInt8] = []
    for y in stride(from: 0, to: height, by: 8) {
        for x in stride(from: 0, to: width, by: 8) { values.append(base[y * rowBytes + x * 4 + 1]) }
    }
    return values
}

/// Text of the speaker's row: the recognized lines after the speaker name,
/// up to the busy indicator or composer placeholder.
func replyText(_ lines: [String], options: Options) -> String? {
    let name = options.speaker
    guard let start = lines.lastIndex(where: { $0.trimmingCharacters(in: .whitespaces) == name })
    else { return nil }
    var reply: [String] = []
    for line in lines[(start + 1)...] {
        if line.contains(options.busy) || line.contains("What do you do?") { break }
        reply.append(line)
    }
    return reply.isEmpty ? nil : reply.joined(separator: " ")
}

func cadence(_ times: [Double], from start: Double, to end: Double) -> [String: Any] {
    let window = times.filter { $0 >= start && $0 <= end }
    let gaps = zip(window.dropFirst(), window).map { $0 - $1 }.sorted()
    let duration = end - start
    return [
        "start_seconds": start,
        "end_seconds": end,
        "frames": window.count,
        "frames_per_second": duration > 0 ? Double(window.count) / duration : 0,
        "median_gap_ms": gaps.isEmpty ? 0 : gaps[gaps.count / 2] * 1000,
        "max_gap_ms": (gaps.last ?? 0) * 1000,
    ]
}

func writePNG(_ image: CIImage, to url: URL, context: CIContext) {
    guard let cgImage = context.createCGImage(image, from: image.extent),
          let destination = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil)
    else { return }
    CGImageDestinationAddImage(destination, cgImage, nil)
    CGImageDestinationFinalize(destination)
}

let options = parseOptions()
let outputURL = URL(fileURLWithPath: options.output, isDirectory: true)
try FileManager.default.createDirectory(at: outputURL, withIntermediateDirectories: true)
let asset = AVURLAsset(url: URL(fileURLWithPath: options.video))
let track = try await asset.loadTracks(withMediaType: .video)[0]
let reader = try AVAssetReader(asset: asset)
let trackOutput = AVAssetReaderTrackOutput(track: track, outputSettings: [
    kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
])
reader.add(trackOutput)
reader.startReading()

var times: [Double] = []
var changed: [Frame] = []
var previous: [UInt8] = []
while let sample = trackOutput.copyNextSampleBuffer() {
    guard let pixels = CMSampleBufferGetImageBuffer(sample) else { continue }
    let time = CMSampleBufferGetPresentationTimeStamp(sample).seconds
    let current = signature(pixels)
    let differs = previous.isEmpty || zip(current, previous).contains { abs(Int($0) - Int($1)) > 12 }
    if differs {
        let image = CIImage(cvPixelBuffer: pixels)
        changed.append(Frame(index: times.count, time: time, lines: try recognizedLines(image), image: image))
    }
    previous = current
    times.append(time)
}
times.sort()
changed.sort { $0.time < $1.time }

let context = CIContext()
var states: [[String: Any]] = []
var lastProvisionalText: String?
var firstBusy: Double?
var firstProvisional: Frame?
var firstFinal: Frame?
for frame in changed {
    let busy = frame.lines.contains { $0.contains(options.busy) }
    if busy, firstBusy == nil { firstBusy = frame.time }
    guard let text = replyText(frame.lines, options: options) else { continue }
    if busy, firstFinal == nil {
        if firstProvisional == nil { firstProvisional = frame }
        // Recognition of the same pixels can vary by a character; compare
        // letters and digits only so noise does not look like a new state.
        let normalized = String(text.lowercased().filter { $0.isLetter || $0.isNumber })
        if normalized != lastProvisionalText {
            let name = String(format: "provisional-%02d-%.3fs.png", states.count, frame.time)
            writePNG(frame.image, to: outputURL.appendingPathComponent(name), context: context)
            states.append(["seconds": frame.time, "frame": frame.index, "characters": text.count, "png": name])
            lastProvisionalText = normalized
        }
    } else if !busy, firstProvisional != nil, firstFinal == nil {
        firstFinal = frame
        writePNG(frame.image, to: outputURL.appendingPathComponent("final.png"), context: context)
    }
}

var report: [String: Any] = [
    "video": options.video,
    "total_frames": times.count,
    "changed_frames": changed.count,
    "provisional_states": states,
    "note": "Reply text is not stored; characters are counted from on-screen text recognition.",
]
if let firstBusy { report["busy_first_seconds"] = firstBusy }
if let firstProvisional { report["provisional_first_seconds"] = firstProvisional.time }
if let firstFinal {
    report["final_seconds"] = firstFinal.time
    report["final_characters"] = replyText(firstFinal.lines, options: options)?.count ?? 0
}
if let firstProvisional, let firstFinal {
    report["provisional_visible_ms"] = (firstFinal.time - firstProvisional.time) * 1000
    report["cadence_streaming"] = cadence(times, from: firstProvisional.time, to: firstFinal.time)
}
if let firstBusy, let end = firstProvisional?.time {
    report["cadence_waiting"] = cadence(times, from: firstBusy, to: end)
}
let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
try data.write(to: outputURL.appendingPathComponent("stream-frames.json"))
print(String(decoding: data, as: UTF8.self))
exit(firstProvisional != nil && firstFinal != nil ? 0 : 1)
