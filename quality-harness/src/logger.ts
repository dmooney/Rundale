export const log = {
  info: (m: string) => console.log(`[quality-harness] ${m}`),
  warn: (m: string) => console.warn(`[quality-harness][warn] ${m}`),
  error: (m: string) => console.error(`[quality-harness][error] ${m}`),
};
