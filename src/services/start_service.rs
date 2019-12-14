use crate::services::{Service, ServiceStatus};
use crate::units::*;

use super::fork_parent;
use super::fork_child;
use super::pre_fork;


fn start_service_with_filedescriptors(
    srvc: &mut Service,
    name: String,
    sockets: ArcMutSocketTable,
    notification_socket_path: std::path::PathBuf,
) {
    // check if executable even exists
    let split: Vec<&str> = match &srvc.service_config {
        Some(conf) => conf.exec.split(' ').collect(),
        None => unreachable!(),
    };

    let cmd = std::path::PathBuf::from(split[0]);
    if !cmd.exists() {
        error!(
            "The service {} specified an executable that does not exist: {:?}",
            name, &cmd
        );
        srvc.status = ServiceStatus::Stopped;
        return;
    }
    if !cmd.is_file() {
        error!(
            "The service {} specified an executable that is not a file: {:?}",
            name, &cmd
        );
        srvc.status = ServiceStatus::Stopped;
        return;
    }

    // 1. fork
    // 2. in fork use dup2 to map all relevant file desrciptors to 3..x
    // 3. in fork mark all other file descriptors with FD_CLOEXEC
    // 4. set relevant env varibales $LISTEN_FDS $LISTEN_PID
    // 4. execve the cmd with the args

    let prefork_res = pre_fork::pre_fork(srvc, &name, &notification_socket_path);

    // make sure we have the lock that the child will need
    let sockets_lock = sockets.lock().unwrap();
    let stream_locked = &*prefork_res.notification_socket.lock().unwrap();
    match nix::unistd::fork() {
        Ok(nix::unistd::ForkResult::Parent { child, .. }) => {
            std::mem::drop(sockets_lock);
            fork_parent::after_fork_parent(
                srvc,
                name,
                child,
                std::path::Path::new(prefork_res.notify_socket_env_var.to_str().unwrap()),
                stream_locked,
            );
        }
        Ok(nix::unistd::ForkResult::Child) => {
            fork_child::after_fork_child(
                srvc,
                &name,
                &*sockets_lock,
                prefork_res.notify_socket_env_var.to_str().unwrap(),
                prefork_res.stdout,
                prefork_res.stderr,
            );
        }
        Err(e) => error!("Fork for service: {} failed with: {}", name, e),
    }
}

pub fn start_service(
    srvc: &mut Service,
    name: String,
    sockets: ArcMutServiceTable,
    notification_socket_path: std::path::PathBuf,
) {
    if let Some(conf) = &srvc.service_config {
        if conf.accept {
            warn!("Inetd style accepting is not supported");
            srvc.status = ServiceStatus::Stopped;
        } else {
            srvc.status = ServiceStatus::Starting;
            start_service_with_filedescriptors(srvc, name, sockets, notification_socket_path);
            srvc.runtime_info.up_since = Some(std::time::Instant::now());
        }
    } else {
        unreachable!();
    }
}