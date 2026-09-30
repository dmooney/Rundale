# Mobile acceptance record

Status: physical-iPhone and human acceptance remain open. Automated package,
simulator, and CI results from #2103 are not device acceptance.

This file is the place to record physical-device sessions once they run. Until
#2046 lands the fuller acceptance tooling from `ios-port`, keep receipts here
or in the PR body with the same fields.

## How to record a session

For each physical session capture:

- source revision (git SHA)
- app marketing/build numbers
- device model and iOS version
- text size / appearance if relevant
- tester and date
- which suites ran (hands-on checklist, automated-on-device, live Endpoint, soak)
- pass / fail / skipped / unavailable per suite
- defects filed

Do not paste Firebase, App Check, or provider secrets into this file or into CI
logs.

## Automated vs device

| Evidence                         | Establishes                                      | Does not establish                |
| -------------------------------- | ------------------------------------------------ | --------------------------------- |
| Swift package tests              | Library behaviour on the Swift toolchain         | UI or device acceptance           |
| Simulator UI (deterministic)     | Fixture/UI contracts on a simulated phone        | Physical interaction / VoiceOver  |
| Live Endpoint (opt-in)           | Authenticated streaming against a real Endpoint  | Human play quality                |
| Soak / performance (opt-in)      | Sustained automation and XCTest metrics          | Approved human responsiveness     |
| Physical iPhone session          | Device acceptance for the exercised checklist    | Future OS/device matrix coverage  |

## Sessions

_None recorded on `main` yet._
