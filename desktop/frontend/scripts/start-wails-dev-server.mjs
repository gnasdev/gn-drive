import { spawn } from "node:child_process";

const port = process.env.WAILS_VITE_PORT || "9245";
const ngBin = process.platform === "win32" ? "node_modules/.bin/ng.cmd" : "node_modules/.bin/ng";

const child = spawn(ngBin, ["serve", "--host", "127.0.0.1", "--port", port], {
  stdio: "inherit",
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});
