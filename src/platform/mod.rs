// src/platform/mod.rs
// Platform abstraction layer for timeout command

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;

// Re-export the platform-specific run function under a common name
#[cfg(unix)]
pub use unix::run_with_timeout;

#[cfg(windows)]
pub use windows::run_with_timeout;
