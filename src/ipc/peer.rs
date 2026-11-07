//! Who is on the other end of a unix-socket connection. The peer pid comes from the kernel,
//! so unlike a `pane` number in the request it cannot be forged by the caller. Walking the
//! parent chain from that pid up to a pane's shell tells which pane the caller runs in.

// Delivery is implemented by the gpui build only; the wgpu binary compiles but never drains.
#![allow(dead_code)]

use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

/// Pid of the process that connected (macOS `LOCAL_PEERPID`).
pub fn peer_pid(stream: &UnixStream) -> Option<u32> {
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: `pid` and `len` are valid for the call and sized as getsockopt expects.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&mut pid as *mut libc::pid_t).cast(),
            &mut len,
        )
    };
    (rc == 0 && pid > 0).then_some(pid as u32)
}

/// Parent pid of `pid`, if the process exists.
fn parent_of(pid: u32) -> Option<u32> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: `info` is a writable proc_bsdinfo of the size passed.
    let n = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (n == size).then_some(info.pbi_ppid)
}

/// `pid` followed by its ancestors, nearest first, stopping at pid 1 or after 64 levels.
pub fn ancestors(pid: u32) -> Vec<u32> {
    let mut chain = vec![pid];
    let mut cur = pid;
    while chain.len() < 64 {
        match parent_of(cur) {
            Some(p) if p > 1 && !chain.contains(&p) => {
                chain.push(p);
                cur = p;
            }
            _ => break,
        }
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_pid_of_a_socket_pair_is_this_process() {
        let (a, _b) = UnixStream::pair().unwrap();
        assert_eq!(peer_pid(&a), Some(std::process::id()));
    }

    #[test]
    fn ancestors_of_a_child_include_this_process() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let chain = ancestors(child.id());
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(chain[0], child_id_or(&chain));
        assert!(chain.contains(&std::process::id()), "{chain:?}");
    }

    fn child_id_or(chain: &[u32]) -> u32 {
        chain[0]
    }

    #[test]
    fn a_connecting_child_is_identified_and_traced_back_to_us() {
        use std::os::unix::net::UnixListener;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.sock");
        let listener = UnixListener::bind(&path).unwrap();
        // a shell -> python chain, like an agent running a tool that runs `petruterm msg`
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("python3 -c 'import socket,sys,time; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); time.sleep(3)' \"$0\"")
            .arg(&path)
            .spawn()
            .unwrap();
        let (stream, _) = listener.accept().unwrap();
        let pid = peer_pid(&stream).expect("peer pid");
        let chain = ancestors(pid);
        let _ = child.kill();
        let _ = child.wait();
        assert_ne!(pid, std::process::id());
        assert!(chain.contains(&std::process::id()), "{chain:?}");
    }

    #[test]
    fn ancestors_of_a_missing_pid_is_just_itself() {
        assert_eq!(ancestors(u32::MAX - 1), vec![u32::MAX - 1]);
    }
}
