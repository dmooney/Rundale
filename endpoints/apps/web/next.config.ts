import type { NextConfig } from "next";

const config: NextConfig = {
  output: "standalone",
  agentRules: false,
  poweredByHeader: false,
  reactStrictMode: true,
};

export default config;
