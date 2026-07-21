<p align="center">
  <!-- Replace with your own image link or relative path -->
  <img src="src/assets/dicktator.gif" alt="Project Logo" width="200" />
</p>

<h1 align="center">dicktator</h1>

<p align="center">
  <b>Free and local wispr flow alternative</b>
</p>

**Dicktator** is a ultra-fast, system-wide voice dictation copyeditor for Windows. It acts as a free and completely offline alternative to paid dictation tools like Wispr Flow. 

Instead of routing your private speech and active application context to cloud servers, Dicktator processes everything **100% locally on your machine**. Powered by a native C++ `whisper.cpp` engine, local LLMs via Ollama, and a lightweight Tauri (Rust) shell, it rewrites your voice input, strips speech disfluencies, handles complex command-mode edits, and injects clean text straight into any application.

---

## Key Features

* **100% Local & Free Alternative to Wispr Flow:** Zero subscription fees, zero cloud API dependencies, zero data leaks. Your microphone audio never leaves your machine.
* **GPU-Accelerated Local STT:** Native integration with `whisper.cpp` using NVIDIA CUDA (`cublas`). Transcribes speech in real time with near-zero latency, bypassing sluggish Python runtimes.
* **Intelligent LLM Post-Processing:** Powered by local Ollama models (`qwen2.5:3b`, `llama3.2:3b`, etc.). It automatically eliminates filler words (`um`, `uh`, `like`), fixes grammar, and resolves self-corrections.
* **Smart Command Mode:** Highlight any text on your screen (in VS Code, Word, Chrome, etc.), press the hotkey, and give a natural voice command (e.g., *"make this tone more professional"* or *"correct the grammar"*). The targeted text is rewritten dynamically in place.
* **Line-Level Context Continuity:** Uses active cursor context tracking (up to the last 300 characters of your current line) so the LLM knows whether to capitalize the next word or insert spaces seamlessly.
* **Custom Vocabulary Injection:** Biases neural speech tokens on the fly using a customizable jargon dictionary for technical terms, code syntax, brand names, and industry jargon.
* **Automated Process Lifecycle:** Silently launches and manages the background C++ inference process on app start, automatically terminating background instances so system VRAM/RAM stays clean.

---

## Requirements & Prerequisites
To run Dicktator locally, ensure your system has the following setup:

* NVIDIA GPU (CUDA-Capable): An RTX/GTX series graphics card with up-to-date graphics drivers.

* Ollama Installed & Running: Download Ollama and pull the lightweight post-processing model:
```bash
ollama run qwen2.5:3b
```
* Node.js & Rust Toolchains: Installed on your machine for running the Tauri frontend and backend hooks.

## 🛠️ Installation & Development Setup
### 1. Clone the Repository
```bash
git clone [https://github.com/sendc0des/dicktator.git]
cd dicktator
npm install
```
### 2. Configure the C++ Engine Binaries
Create a folder named bin inside src-tauri/:

```bash
mkdir -p src-tauri/bin
```
Place the following files directly inside src-tauri/bin/:

* whisper-server.exe (Compiled whisper.cpp server)

* ggml-large-v3-turbo-q5_0.bin (Whisper GGUF Model)

* Required CUDA .dll files (ggml-cuda.dll, cudart64_12.dll, cublas64_12.dll, etc.)

### 3. Run the Development Server
```bash
npm run tauri dev
```
## How to Use
* Standard Dictation: Place your cursor into any text box (Notepad, VS Code, Discord, Browser), hold down Ctrl + Space, speak naturally, and release. The formatted text will type directly into the focused app.

* Command Mode (Text Rewriting): Highlight any text with your mouse/keyboard, hold Ctrl + Space, speak a command (e.g., "convert this into a bulleted list" or "fix the typos"), and release. The selected text will be replaced automatically.

## Customizing the Phrase Dictionary
To bias the engine toward specific acronyms or unique names, open src-tauri/src/lib.rs and update the dictionary string inside process_with_local_whisper:

```rust
let dictionary = "Dicktator, Tauri, Rust, Ollama, whisper.cpp, GitHub, VS Code";
```