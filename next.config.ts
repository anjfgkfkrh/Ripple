import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Docker web 이미지는 .next/standalone만 복사해 가볍게 만든다.
  output: "standalone",
  cacheComponents: true,
  partialPrefetching: true,
  turbopack: {
    rules: {
      "*.css": {
        loaders: ["@tailwindcss/turbopack"],
        as: "*.css",
      },
    },
  },
};

export default nextConfig;
