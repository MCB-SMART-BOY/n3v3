//! Common utilities and data structures for n3v3.
//! n3v3 的通用工具和数据结构。
//!
//! This crate provides foundational types used across the n3v3 compiler:
//! 本 crate 提供 n3v3 编译器中使用的基础类型：
//!
//! - `Span`: Source code location tracking / 源码位置跟踪
//! - `Interner`: String interning for efficient symbol handling / 字符串驻留，用于高效的符号处理
//! - `Arena`: Memory arena for AST allocation / 内存池，用于 AST 分配

mod action;
mod int;
mod interner;
mod span;
mod trivia;

pub use action::{
    ActionPlan, EffectSummary, FakeHost, Host, HostError, HostOp, HostOpKind, HostValue,
    IntrinsicMetadata, OsHost, ProcessPlan, ProcessRedirect, ProcessStage, ProcessStream,
    intrinsic_metadata, is_effectful_builtin,
};
pub use int::{
    Int, int_abs, int_from_f64, int_is_negative, int_is_zero, int_to_f64, int_to_i64, int_to_u32,
    int_to_usize, parse_int, parse_int_radix,
};
pub use interner::{Interner, Symbol};
pub use span::{BytePos, Span};
pub use trivia::{Comment, CommentKind};

/// Kill a process by PID using the platform-appropriate mechanism.
/// This is the SINGLE SOURCE OF TRUTH for process termination across all crates.
/// Used by both n3v3-std (awaitTaskWithTimeout) and n3v3-eval (streaming timeout).
///
/// Unix: sends SIGKILL via libc::kill
/// Windows: uses taskkill /F /PID
pub fn kill_process(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
    }
}
