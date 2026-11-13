use std::fs;
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::Value;

struct Core(Child);

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(path: &Path) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(text) = fs::read_to_string(path) {
            return serde_json::from_str(&text).expect("the connection file is JSON");
        }
        assert!(
            Instant::now() < deadline,
            "no connection file appeared at {}",
            path.display()
        );
        sleep(Duration::from_millis(50));
    }
}

#[test]
fn serve_writes_a_connection_file_naming_the_port_it_listens_on_and_its_token() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("serve_command")
        .join("connection.json");
    let _ = fs::remove_file(&path);
    let _core = Core(
        Command::new(env!("CARGO_BIN_EXE_resonance"))
            .arg("serve")
            .arg("--connection-file")
            .arg(&path)
            .stdout(Stdio::null())
            .spawn()
            .expect("the resonance binary starts"),
    );

    let connection = wait_for(&path);

    let port = u16::try_from(connection["port"].as_u64().unwrap()).unwrap();
    assert!(TcpStream::connect(("127.0.0.1", port)).is_ok());
    assert_eq!(connection["token"].as_str().unwrap().len(), 64);
}
