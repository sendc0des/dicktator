const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const pill = document.getElementById('pill');
let isRecording = false;

listen('shortcut-pressed', async () => {
  if (isRecording) return; 
  isRecording = true;

  pill.classList.remove('processing');
  pill.classList.add('listening');

  try {
    await invoke('start_recording');
  } catch (e) {
    console.error("Failed to start:", e);
  }
});

listen('shortcut-released', async () => {
  if (!isRecording) return;
  
  // Remove the listening wave immediately for a clean transition
  pill.classList.remove('listening');

  try {
    // We just trigger the stop command and let Rust handle the states
    await invoke('stop_recording');
  } catch (e) {
    console.error("Failed to process:", e);
  }

  // Clean reset
  isRecording = false;
  pill.classList.remove('processing');
});

// NEW: Only show the dots if the backend explicitly tells us there is valid speech!
listen('started-processing', () => {
  pill.classList.add('processing');
});