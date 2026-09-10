import pino, { type LoggerOptions } from "pino";

export const redactPaths = [
  "req.headers.authorization",
  "req.headers.cookie",
  "headers.authorization",
  "apiKey",
  "secret",
  "providerApiKey",
  "instructions",
  "input",
  "output",
  "image",
];

export function createLoggerOptions(options: LoggerOptions = {}): LoggerOptions {
  return {
    level: process.env.LOG_LEVEL ?? "info",
    redact: { paths: redactPaths, censor: "[REDACTED]" },
    base: { service: "parish-endpoints" },
    ...options,
  };
}

export function createLogger(options: LoggerOptions = {}) {
  return pino(createLoggerOptions(options));
}
