import react from '@vitejs/plugin-react';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  // 打包成单个 HTML 文件：命令行会把它嵌进程序，生成页面时再写入讲解数据
  plugins: [react(), viteSingleFile()],
  // 示例数据放在仓库根目录的 examples/，开发时可以直接访问 /walkthrough.json
  publicDir: '../examples',
  server: { port: 5173, strictPort: true },
  build: { copyPublicDir: false },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
  },
});
