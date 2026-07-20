# 🎙️ Dicktator 

An elite, low-latency, system-wide local voice dictation copyeditor built with **Tauri**, **Rust**, and **Natively Accelerated C++**. 

Dicktator listens to your speech globally via a custom hardware shortcut (`Ctrl + Space`), feeds raw audio directly into your local GPU using an embedded `whisper.cpp` server, and passes the transcript to a localized LLM (`qwen2.5:3b`) to resolve self-corrections, stutters, and apply seamless line-level context formatting directly into your active window.

---

## ⚡ Features

*   **GPU-Accelerated Local STT:** Seamless integration with `whisper.cpp` using NVIDIA CUDA (`cublas`) execution. Audio processing takes milliseconds, bypassing sluggish Python runtimes.
*   **Intelligent Post-Processing:** Powered by Ollama (`qwen2.5:3b` / `llama3.2:3b`). It automatically clears out verbal fillers (`um`, `uh`, `like`), fixes grammar, and resolves real-time self-corrections (e.g., *"Let's meet at 5... no wait, make it 6"* outputting precisely as *"Let's meet at 6."*).
*   **Line-Level Context Awareness:** Captures up to the last 300 characters of your current cursor line dynamically using native hardware macros, ensuring perfect punctuation continuity and trailing space logic.
*   **Custom Vocabulary Injection:** Bias neural tokens on the fly using the integrated custom phrase dictionary for seamless industry jargon, code notation, and custom brand name spelling.
*   **Zero-Config Engine Lifecycle:** The background C++ Whisper inference server automatically boots in a silent, windowless state when the app initializes and cleans up perfectly upon termination.

---

🚀 Getting Started
Prerequisites
Ollama Runtime: Ensure Ollama is installed and running locally with the target model:

Bash
ollama run qwen2.5:3b
Rust & Node Toolchains: Ensure your machine has Cargo and Node.js setup for Tauri development.

NVIDIA CUDA Toolchain: Windows users require a CUDA-capable GPU (GeForce RTX series) with updated graphics drivers.

Local Installation & Dev Mode
Clone the repository and install dependency nodes:

Bash
git clone [https://github.com/YOUR_USERNAME/dicktator.git](https://github.com/YOUR_USERNAME/dicktator.git)
cd dicktator
npm install
Set up the local C++ inference engines inside your Tauri workspace:

Create the directory structure: src-tauri/bin/

Place your compiled whisper-server.exe, target .bin model file (ggml-large-v3-turbo-q5_0.bin), and all corresponding architecture .dll files (ggml-cuda.dll, cudart64_12.dll, etc.) directly inside that folder.

Boot the execution engine:

Bash
npm run tauri dev
