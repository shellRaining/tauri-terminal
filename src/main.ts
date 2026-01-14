import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const terminalElement = document.getElementById("terminal") as HTMLElement;

const fitAddon = new FitAddon();
const term = new Terminal({
  fontFamily: "JetBrainsMono Nerd Font Mono, monospace",
  theme: {
    background: "rgb(47, 47, 47)",
  },
});
term.loadAddon(fitAddon);
term.open(terminalElement);

// Make the terminal fit all the window size
async function fitTerminal() {
  fitAddon.fit();
  void invoke<string>("async_resize_pty", {
    rows: term.rows,
    cols: term.cols,
  });
}

// Write data from pty into the terminal
function writeToTerminal(data: string) {
  return new Promise<void>((r) => {
    term.write(data, () => r());
  });
}

let pendingOutput = "";
let flushScheduled = false;
let flushInProgress = false;

function scheduleFlush() {
  if (flushScheduled) return;
  flushScheduled = true;

  window.requestAnimationFrame(() => {
    flushScheduled = false;
    void flushOutput();
  });
}

async function flushOutput() {
  if (flushInProgress) return;
  if (!pendingOutput) return;

  flushInProgress = true;
  const chunk = pendingOutput;
  pendingOutput = "";
  await writeToTerminal(chunk);
  flushInProgress = false;

  if (pendingOutput) scheduleFlush();
}

// Write data from the terminal to the pty
function writeToPty(data: string) {
  void invoke("async_write_to_pty", {
    data,
  });
}

function initShell() {
  invoke("async_create_shell").catch((error) => {
    // on linux it seem to to "Operation not permitted (os error 1)" but it still works because echo $SHELL give /bin/bash
    console.error("Error creating shell:", error);
  });
}

initShell();
term.onData(writeToPty);
addEventListener("resize", fitTerminal);
fitTerminal();

listen<string>("pty:data", async (event) => {
  if (event.payload) {
    pendingOutput += event.payload;
    scheduleFlush();
  }
}).catch((error) => {
  console.error("Error listening to pty:data:", error);
});

listen<string>("pty:error", (event) => {
  console.error("PTY error:", event.payload);
}).catch((error) => {
  console.error("Error listening to pty:error:", error);
});

async function initWebgl() {
  try {
    const mod = await import("@xterm/addon-webgl");
    const webglAddon = new mod.WebglAddon();
    term.loadAddon(webglAddon);
  } catch (error) {
    console.warn("WebGL addon unavailable, falling back to canvas:", error);
  }
}

void initWebgl();
