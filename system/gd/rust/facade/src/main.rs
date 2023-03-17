//! Starts the facade services that allow us to test the Bluetooth stack

use bluetooth_with_facades::RootFacadeService;
use clap::{value_parser, Arg, Command};
use futures::channel::mpsc;
use futures::executor::block_on;
use futures::stream::StreamExt;
use grpcio::*;
use lazy_static::lazy_static;
use log::debug;
use nix::sys::signal;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

fn main() {
    let sigint = install_sigint();
    bt_common::init_logging();
    let rt = Arc::new(Runtime::new().unwrap());
    rt.block_on(async_main(Arc::clone(&rt), sigint));
}

async fn async_main(rt: Arc<Runtime>, mut sigint: mpsc::UnboundedReceiver<()>) {
    let matches = Command::new("bluetooth_with_facades")
        .about("The bluetooth stack, with testing facades enabled and exposed via gRPC.")
        .arg(
            Arg::new("root-server-port")
                .long("root-server-port")
                .value_parser(value_parser!(u16))
                .default_value("8897"),
        )
        .arg(
            Arg::new("grpc-port")
                .long("grpc-port")
                .value_parser(value_parser!(u16))
                .default_value("8899"),
        )
        .arg(
            Arg::new("signal-port")
                .long("signal-port")
                .value_parser(value_parser!(u16))
                .default_value("8895"),
        )
        .arg(Arg::new("rootcanal-port").long("rootcanal-port").value_parser(value_parser!(u16)))
        .arg(Arg::new("btsnoop").long("btsnoop"))
        .arg(Arg::new("btsnooz").long("btsnooz"))
        .arg(Arg::new("btconfig").long("btconfig"))
        .get_matches();

    let root_server_port = *matches.get_one::<u16>("root-server-port").unwrap();
    let grpc_port = *matches.get_one::<u16>("grpc-port").unwrap();
    let rootcanal_port = matches.get_one::<u16>("rootcanal-port").copied();
    let env = Arc::new(Environment::new(2));
    let mut server = ServerBuilder::new(env)
        .register_service(RootFacadeService::create(
            rt,
            grpc_port,
            rootcanal_port,
            matches.get_one::<String>("btsnoop").cloned(),
        ))
        .bind("0.0.0.0", root_server_port)
        .build()
        .unwrap();
    server.start();

    sigint.next().await;
    block_on(server.shutdown()).unwrap();
}

// TODO: remove as this is a temporary nix-based hack to catch SIGINT
fn install_sigint() -> mpsc::UnboundedReceiver<()> {
    let (tx, rx) = mpsc::unbounded();
    *SIGINT_TX.lock().unwrap() = Some(tx);

    let sig_action = signal::SigAction::new(
        signal::SigHandler::Handler(handle_sigint),
        signal::SaFlags::empty(),
        signal::SigSet::empty(),
    );
    unsafe {
        signal::sigaction(signal::SIGINT, &sig_action).unwrap();
    }

    rx
}

lazy_static! {
    static ref SIGINT_TX: Mutex<Option<mpsc::UnboundedSender<()>>> = Mutex::new(None);
}

extern "C" fn handle_sigint(_: i32) {
    let mut sigint_tx = SIGINT_TX.lock().unwrap();
    if let Some(tx) = &*sigint_tx {
        debug!("Stopping gRPC root server due to SIGINT");
        tx.unbounded_send(()).unwrap();
    }
    *sigint_tx = None;
}
