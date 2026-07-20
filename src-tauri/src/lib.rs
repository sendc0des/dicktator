use tauri::State;
use std::sync::{Arc, Mutex};
use hound;
use reqwest;
use serde_json::json;
use base64::{Engine as _, engine::general_purpose};
use tauri::Emitter;
use tauri::Manager;
use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::Foundation::MAX_PATH;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, GetWindowTextW};
use std::os::windows::process::CommandExt;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use std::collections::VecDeque;

struct AppState {
    is_recording: Arc<Mutex<bool>>,
    audio_data: Arc<Mutex<Vec<i16>>>,
    sample_rate: Arc<Mutex<u32>>,
    channels: Arc<Mutex<u16>>,
    ring_buffer: Arc<Mutex<std::collections::VecDeque<i16>>>
}

#[tauri::command]
fn start_recording(state: State<AppState>) -> Result<(), String> {
    let mut is_recording = state.is_recording.lock().unwrap();
    if *is_recording { return Err("Already recording".into()); }
    
    // Back to basic, rapid audio initialization
    let mut audio_data = state.audio_data.lock().unwrap();
    audio_data.clear();
    
    let mut ring = state.ring_buffer.lock().unwrap();
    audio_data.extend(ring.iter());
    ring.clear(); 
    
    *is_recording = true;
    Ok(())
}

// Helper function to talk asynchronously to our local Ollama server
// Set to true to use OpenRouter, false to use local Whisper CLI + Ollama
const USE_CLOUD_API: bool = false; 

// (Keep your existing process_with_openrouter function here)

async fn process_with_local_whisper(file_path: &std::path::Path) -> Result<String, String> {
    println!("🚀 Sending audio to local C++ Whisper server...");
    let client = reqwest::Client::new();
    
    let audio_bytes = std::fs::read(file_path).map_err(|e| e.to_string())?;
    
    let part = reqwest::multipart::Part::bytes(audio_bytes)
        .file_name("dictation.wav")
        .mime_str("audio/wav")
        .unwrap();
        
    // --- THE HALLUCINATION FIX ---
    // We explicitly lock the engine to English and disable creative decoding
    let dictionary = "Dicktator, Tauri, Rust, Ollama, whisper.cpp, GitHub, frontend, backend";
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("language", "en")          // Force English to stop translation hallucinations
        .text("temperature", "0.0")      // Force deterministic, greedy decoding
        .text("response_format", "json");

    let stt_res = client.post("http://127.0.0.1:8080/inference")
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Local STT failed: {}", e))?;

    let stt_json: serde_json::Value = stt_res.json().await.map_err(|e| e.to_string())?;
    let raw_transcript = stt_json["text"].as_str().unwrap_or("").trim().to_string();
    
    println!("📝 Local Whisper Transcript: \"{}\"", raw_transcript);
    
    Ok(raw_transcript)
}

async fn process_with_ollama(system_prompt: &str, user_content: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    
    // Clean dynamic integration for Ollama text-generation endpoint
    let combined_prompt = format!(
        "System Instructions:\n{}\n\nUser Input Context:\n{}\n\nFinal Output:", 
        system_prompt, 
        user_content
    );

    let payload = serde_json::json!({
        "model": "qwen2.5:3b", 
        "prompt": combined_prompt,
        "stream": false,
        "keep_alive": -1,
        "options": { "temperature": 0.0 }
    });

    let response = client.post("http://localhost:11434/api/generate")
        .json(&payload).send().await
        .map_err(|e| format!("Failed to reach Ollama: {}", e))?;

    let res_json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    
    if let Some(final_text) = res_json["response"].as_str() {
        Ok(final_text.trim().to_string())
    } else {
        Err("Invalid response structure".into())
    }
}

// IMPORTANT: Replace this with your actual OpenRouter API Key
const OPENROUTER_API_KEY: &str = "";

async fn process_with_openrouter(file_path: &std::path::Path, system_prompt: &str, user_content: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    
    // --- PHASE 1: WHISPER TRANSCRIPTION ---
    let audio_bytes = std::fs::read(file_path).map_err(|e| format!("Failed to read audio: {}", e))?;
    let base64_audio = general_purpose::STANDARD.encode(audio_bytes);

    let stt_payload = json!({
        "model": "openai/whisper-large-v3",
        "input_audio": { "data": base64_audio, "format": "wav" }
    });

    let stt_res = client.post("https://openrouter.ai/api/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", OPENROUTER_API_KEY))
        .json(&stt_payload).send().await
        .map_err(|e| format!("OpenRouter STT request failed: {}", e))?;

    let stt_json: serde_json::Value = stt_res.json().await.map_err(|e| e.to_string())?;
    let raw_transcript = stt_json["text"].as_str().unwrap_or("").trim().to_string();
    
    if raw_transcript.is_empty() { return Ok("".to_string()); }

    // --- PHASE 2: DYNAMIC INTENT COMPILATION ---
    let final_user_message = user_content.replace("{VOICE_COMMAND}", &raw_transcript);

    let llm_payload = json!({
        "model": "meta-llama/llama-3.1-8b-instruct",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": final_user_message}
        ],
        "temperature": 0.0
    });

    let llm_res = client.post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", OPENROUTER_API_KEY))
        .json(&llm_payload).send().await
        .map_err(|e| format!("OpenRouter LLM request failed: {}", e))?;

    let llm_json: serde_json::Value = llm_res.json().await.map_err(|e| e.to_string())?;
    
    if let Some(final_text) = llm_json["choices"][0]["message"]["content"].as_str() {
        Ok(final_text.trim().to_string())
    } else {
        Err("Invalid LLM response structure".into())
    }
}

fn get_active_app_context() -> (String, String) {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() { return ("unknown".to_string(), "unknown".to_string()); }

        // 1. Get Window Title
        let mut title_buffer = vec![0u16; 512];
        let title_len = GetWindowTextW(hwnd, title_buffer.as_mut_ptr(), title_buffer.len() as i32);
        let window_title = if title_len > 0 {
            String::from_utf16_lossy(&title_buffer[..title_len as usize]).to_lowercase()
        } else {
            "unknown".to_string()
        };

        // 2. Get Executable Name
        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id == 0 { return ("unknown".to_string(), window_title); }

        let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process_handle.is_null() { return ("unknown".to_string(), window_title); }

        let mut buffer = vec![0u16; MAX_PATH as usize];
        let mut size = buffer.len() as u32;
        let success = QueryFullProcessImageNameW(process_handle, 0, buffer.as_mut_ptr(), &mut size);
        
        if success != 0 {
            let path_os = String::from_utf16_lossy(&buffer[..size as usize]);
            if let Some(filename) = std::path::Path::new(&path_os).file_name() {
                return (filename.to_string_lossy().to_string().to_lowercase(), window_title);
            }
        }
        ("unknown".to_string(), window_title)
    }
}

#[tauri::command]
async fn stop_recording(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<String, String> {
    // 1. ADVANCED RMS SILENCE INTERCEPTOR
    let is_silent = {
        let raw_audio = state.audio_data.lock().unwrap();
        if raw_audio.is_empty() {
            true
        } else {
            let mut sum_squares: f64 = 0.0;
            for &sample in raw_audio.iter() {
                let s = sample as f64;
                sum_squares += s * s;
            }
            let rms = (sum_squares / raw_audio.len() as f64).sqrt();
            println!("🎤 Detected Mic Volume (RMS): {:.1}", rms);
            rms < 1000.0 
        }
    };

    if is_silent {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
        let mut is_recording = state.is_recording.lock().unwrap();
        *is_recording = false;
        return Ok("".to_string());
    }

    // 2. VALID AUDIO DETECTED -> Signal frontend UI dots
    let _ = app.emit("started-processing", ());

    // --- 3. UPGRADED LINE-LEVEL CONTEXT MACRO ---
    let mut command_mode_text = String::new();
    let mut preceding_text = String::new();
    
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let original_clipboard = clipboard.get_text().unwrap_or_default();
        
        if let Ok(mut virtual_keyboard) = enigo::Enigo::new(&enigo::Settings::default()) {
            use enigo::{Keyboard, Key, Direction};
            
            // 1. Check for highlighted text (Command Mode)
            let _ = virtual_keyboard.key(Key::Control, Direction::Press);
            let _ = virtual_keyboard.key(Key::Unicode('c'), Direction::Click);
            let _ = virtual_keyboard.key(Key::Control, Direction::Release);
            
            std::thread::sleep(std::time::Duration::from_millis(50)); 
            let new_clipboard = clipboard.get_text().unwrap_or_default();
            
            if new_clipboard != original_clipboard && !new_clipboard.is_empty() {
                command_mode_text = new_clipboard.trim().to_string();
            } else {
                // 2. No highlight -> Grab the ENTIRE current line up to the cursor safely
                let _ = virtual_keyboard.key(Key::Shift, Direction::Press);
                let _ = virtual_keyboard.key(Key::Home, Direction::Click);
                let _ = virtual_keyboard.key(Key::Shift, Direction::Release);
                
                let _ = virtual_keyboard.key(Key::Control, Direction::Press);
                let _ = virtual_keyboard.key(Key::Unicode('c'), Direction::Click);
                let _ = virtual_keyboard.key(Key::Control, Direction::Release);
                
                // Clear selection by hitting Right Arrow
                let _ = virtual_keyboard.key(Key::RightArrow, Direction::Click);
                
                std::thread::sleep(std::time::Duration::from_millis(50)); 
                if let Ok(text) = clipboard.get_text() {
                    // Truncate to the last 300 characters to keep prompt small and dense
                    let len = text.len();
                    let safe_len = if len > 300 { 300 } else { len };
                    preceding_text = text[len - safe_len..].to_string();
                }
            }
        }
        
        // Restore original clipboard contents seamlessly
        let _ = clipboard.set_text(original_clipboard);
    }

    // Flush out the remaining active hardware audio buffer frames safely
    std::thread::sleep(std::time::Duration::from_millis(400));
    
    // --- 4. DYNAMIC PROMPT SYSTEM MODE SPLITTING ---
    let (active_app, window_title) = get_active_app_context();
    println!("🔌 Context Captured -> App: {}, Title: {}", active_app, window_title);

    let (system_prompt, user_content) = if !command_mode_text.is_empty() {
        println!("🪄 Command Mode Activated for text: \"{}\"", command_mode_text);
        
        let sys = "You are an elite text-editing engine operating in COMMAND MODE. \
                   You are provided with a segment of highlighted text and a spoken voice command. \
                   Your single goal is to execute the instructions of the voice command directly onto the highlighted text. \
                   Strict Rules: \
                   1. Output ONLY the finalized, rewritten text. \
                   2. Do NOT include explanations, notes, conversational replies, markdown, or wrap code blocks in fences. \
                   3. If the voice command specifies a grammar correction (e.g., 'correct the grammar'), output the modified sentence cleanly without altering vocabulary meaning.";

        let user = format!(
            "Target App Context: '{}' ({})\n\
             Highlighted Text to Modify:\n\"{}\"\n\n\
             Voice Command Instruction:\n\"{{VOICE_COMMAND}}\"",
            active_app, window_title, command_mode_text
        );
        
        (sys.to_string(), user)
    } else {
        // Standard high-performance dictation mode
        let sys = "You are an elite voice dictation post-processor. \
                   Your entire job is to convert raw speech transcripts into clean, natural text according to the current context. \
                   Rules: \
                   1. Preserve meaning exactly; never summarize or paraphrase. \
                   2. Fix grammar, spelling, capitalization, punctuation and spacing. \
                   3. Strip speech disfluencies (um, uh, like) and resolve self-corrections/backtracking seamlessly. \
                   4. Do NOT output notes, explanations, markdown, or quotation marks unless they are required and make sense. Output raw text ONLY.";

        let user = if preceding_text.is_empty() {
            "The user is typing a new sentence.\nRaw Audio Transcript: \"{VOICE_COMMAND}\"".to_string()
        } else {
            format!(
                "Preceding line text already written: \"{}\"\n\
                 CRITICAL RULE: Do NOT reproduce the preceding line text. Use it only to apply context-aware spacing and capitalization rules.\n\
                 Raw Audio Transcript: \"{{VOICE_COMMAND}}\"",
                preceding_text
            )
        };
        
        (sys.to_string(), user)
    };

    // Wait a brief moment for the hardware stream to flush out gracefully
    std::thread::sleep(std::time::Duration::from_millis(400));
    
    let processed_audio = {
        let mut is_recording = state.is_recording.lock().unwrap();
        *is_recording = false;
        
        let raw_audio = state.audio_data.lock().unwrap().clone();
        let native_sr = *state.sample_rate.lock().unwrap();
        let native_ch = *state.channels.lock().unwrap() as usize;
        
        // --- CHANNEL MIXING ---
        let mut mono_audio = Vec::new();
        if native_ch > 1 {
            for chunk in raw_audio.chunks(native_ch) {
                if chunk.len() == native_ch {
                    let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                    mono_audio.push((sum / native_ch as i32) as i16);
                }
            }
        } else {
            mono_audio = raw_audio;
        }

        let mut f32_mono: Vec<f32> = mono_audio.iter().map(|&s| s as f32 / (i16::MAX as f32)).collect();
        
        let mut max_amplitude: f32 = 0.0;
        for &sample in &f32_mono {
            if sample.abs() > max_amplitude {
                max_amplitude = sample.abs();
            }
        }
        
        if max_amplitude > 0.0 && max_amplitude < 0.9 {
            let gain: f32 = 0.9 / max_amplitude;
            for sample in &mut f32_mono { *sample *= gain; }
        }

        // --- DOWNSAMPLING ---
        let target_sr = 16000;
        let mut final_audio = Vec::new();
        
        if native_sr != target_sr {
            use rubato::{Resampler, SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};
            let target_len = f32_mono.len() + 1024;
            f32_mono.resize(target_len, 0.0);
            
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            
            let mut resampler = SincFixedIn::<f32>::new(
                target_sr as f64 / native_sr as f64, 2.0, params, f32_mono.len(), 1,
            ).map_err(|e| e.to_string())?;

            let output = resampler.process(&vec![f32_mono], None).map_err(|e| e.to_string())?;
            
            for &sample in &output[0] {
                let clamped: f32 = sample.clamp(-1.0_f32, 1.0_f32);
                final_audio.push((clamped * (i16::MAX as f32)) as i16);
            }
        } else {
            for &sample in &f32_mono {
                let clamped: f32 = sample.clamp(-1.0_f32, 1.0_f32);
                final_audio.push((clamped * (i16::MAX as f32)) as i16);
            }
        }
        
        final_audio
    };
    
    // --- FILE CREATION ---
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("dicktator_raw.wav");
    
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000, 
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    
    let mut writer = hound::WavWriter::create(&file_path, spec).map_err(|e| e.to_string())?;
    for &sample in processed_audio.iter() {
        writer.write_sample(sample).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    
    // --- 5. PIPELINE ROUTING ---
    let polished_text = if USE_CLOUD_API {
        process_with_openrouter(&file_path, &system_prompt, &user_content).await?
    } else {
        let raw_transcript = process_with_local_whisper(&file_path).await?;
        
        if raw_transcript.is_empty() {
            "".to_string()
        } else {
            println!("🧠 Formatting transcript with local Ollama engine...");
            // Swap out placeholder for local LLM consumption
            let final_user_content = user_content.replace("{VOICE_COMMAND}", &raw_transcript);
            process_with_ollama(&system_prompt, &final_user_content).await?
        }
    };

    // Hide the window right before executing keyboard input focus
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    // --- SYSTEM-WIDE KEYBOARD INJECTION & HALLUCINATION FILTER ---
    // 1. Remove all newlines so Enigo never accidentally presses "Enter"
    let safe_text = polished_text.replace('\n', " ").replace('\r', "");
    let text_to_type = safe_text.trim().to_string();
    
    let is_hallucination = (text_to_type.starts_with('(') && text_to_type.ends_with(')')) 
                        || (text_to_type.starts_with('[') && text_to_type.ends_with(']'));

    if is_hallucination {
        println!("🛡️ Blocked text injection: Output flagged as a music/ambient hallucination.");
    }

    if !text_to_type.is_empty() && !is_hallucination {
        println!("⌨️ Injecting text via virtual hardware: \"{}\"", text_to_type);
        std::thread::spawn(move || {
            use enigo::{Enigo, Keyboard, Settings};
            std::thread::sleep(std::time::Duration::from_millis(150)); 
            if let Ok(mut virtual_keyboard) = Enigo::new(&Settings::default()) {
                let _ = virtual_keyboard.text(&text_to_type);
            }
        }); 
    }
    
    Ok(polished_text)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Initialize the global shortcut plugin
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            is_recording: std::sync::Arc::new(std::sync::Mutex::new(false)),
            audio_data: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            sample_rate: std::sync::Arc::new(std::sync::Mutex::new(16000)),
            channels: std::sync::Arc::new(std::sync::Mutex::new(1)),
            ring_buffer: std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
        })
        .setup(|app| {
            // --- 1. LOCAL WHISPER SERVER AUTO-BOOT ---
            let current_dir = std::env::current_dir().unwrap();
            let server_path = current_dir.join("bin").join("whisper-server.exe");
            let model_path = current_dir.join("bin").join("ggml-large-v3-turbo-q5_0.bin");
            // Forces Windows to kill any orphaned ghost instances before booting a fresh one
            let _ = std::process::Command::new("taskkill")
                .args(&["/F", "/IM", "whisper-server.exe"])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .status();

            std::thread::spawn(move || {
                use std::os::windows::process::CommandExt;
                let _child = std::process::Command::new(server_path)
                    .arg("-m").arg(model_path)
                    .arg("--host").arg("127.0.0.1")
                    .arg("--port").arg("8080")
                    .creation_flags(0x08000000) // CREATE_NO_WINDOW
                    .spawn()
                    .unwrap_or_else(|e| {
                        println!("Failed to start automatic whisper-server process: {}", e);
                        panic!("{}", e);
                    });
            });

            // --- 2. AUDIO STREAM INITIALIZATION ---
            use tauri::Manager;
            let app_state = app.state::<AppState>();
            let ring_buffer_clone = std::sync::Arc::clone(&app_state.ring_buffer);
            let is_recording_clone = std::sync::Arc::clone(&app_state.is_recording);
            let audio_data_clone = std::sync::Arc::clone(&app_state.audio_data);

            let host = cpal::default_host();
            let device = host.default_input_device().expect("Failed to get default input device");
            let config = device.default_input_config().expect("Failed to get default input config");
            
            *app_state.sample_rate.lock().unwrap() = config.sample_rate().0;
            *app_state.channels.lock().unwrap() = config.channels();

            std::thread::spawn(move || {
                use cpal::traits::{DeviceTrait, StreamTrait};
                let err_fn = |err| eprintln!("Audio stream error: {}", err);
                
                let stream = match config.sample_format() {
                    cpal::SampleFormat::F32 => {
                        device.build_input_stream(
                            &config.into(),
                            move |data: &[f32], _: &_| {
                                let is_rec = *is_recording_clone.lock().unwrap();
                                if is_rec {
                                    let i16_data: Vec<i16> = data.iter().map(|&s| (s * i16::MAX as f32) as i16).collect();
                                    audio_data_clone.lock().unwrap().extend(i16_data);
                                } else {
                                    let mut rb = ring_buffer_clone.lock().unwrap();
                                    for &sample in data {
                                        if rb.len() >= 16000 * 2 { rb.pop_front(); }
                                        rb.push_back((sample * i16::MAX as f32) as i16);
                                    }
                                }
                            },
                            err_fn, None
                        ).expect("Failed to build F32 audio stream")
                    },
                    cpal::SampleFormat::I16 => {
                        device.build_input_stream(
                            &config.into(),
                            move |data: &[i16], _: &_| {
                                let is_rec = *is_recording_clone.lock().unwrap();
                                if is_rec {
                                    audio_data_clone.lock().unwrap().extend_from_slice(data);
                                } else {
                                    let mut rb = ring_buffer_clone.lock().unwrap();
                                    for &sample in data {
                                        if rb.len() >= 16000 * 2 { rb.pop_front(); }
                                        rb.push_back(sample);
                                    }
                                }
                            },
                            err_fn, None
                        ).expect("Failed to build I16 audio stream")
                    },
                    _ => panic!("Unsupported microphone sample format!"),
                };

                stream.play().expect("Failed to start audio stream");
                println!("🎤 Microphone successfully initialized and listening.");
                loop { std::thread::sleep(std::time::Duration::from_secs(3600)); }
            });

            // --- 3. RESTORE RUST-SIDE GLOBAL SHORTCUT LISTENER ---
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
            
            // This forces the OS to catch "Ctrl+Space" globally
            app.global_shortcut().on_shortcut("ctrl+space", move |app_handle, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    
                    // 1. Unhide the window so the user can see the UI
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                    }
                    
                    // 2. Trigger main.js -> adds 'listening' class & invokes start_recording
                    let _ = app_handle.emit("shortcut-pressed", ());
                    
                } else if event.state == ShortcutState::Released {
                    
                    // 3. Trigger main.js -> invokes stop_recording & resets UI
                    let _ = app_handle.emit("shortcut-released", ());
                    
                }
            }).unwrap_or_else(|e| println!("Failed to register hotkey: {}", e));    

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}