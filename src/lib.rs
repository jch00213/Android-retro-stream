use jni::objects::{JClass, JByteArray};
use jni::sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use std::net::UdpSocket;
use std::sync::{Mutex, RwLock};

struct StreamingServer {
    socket: UdpSocket,
    target_addr: String,
}

// Use an RwLock or Mutex option so we can safely tear down and re-bind across stream sessions
static SERVER: RwLock<Option<Mutex<StreamingServer>>> = RwLock::new(None);

/// Initializes the UDP socket bound to a local port
#[no_mangle]
pub extern "C" fn Java_com_jeremy_stream_NativeBridge_initServer(
    _env: JNIEnv,
    _class: JClass,
    port: jint,
) -> jboolean {
    let bind_addr = format!("0.0.0.0:{}", port);
    
    // Bind the socket
    let socket = match UdpSocket::bind(&bind_addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to bind UDP socket: {}", e);
            return JNI_FALSE;
        }
    };

    let _ = socket.set_nonblocking(true);
    
    let server = StreamingServer {
        socket,
        target_addr: "192.168.49.1:9000".to_string(),
    };

    // Safely overwrite or set the global state on every start request
    if let Ok(mut guard) = SERVER.write() {
        *guard = Some(Mutex::new(server));
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

/// Receives raw AV1 encoded frames from Kotlin, wraps them with framing metadata, and sends via UDP
#[no_mangle]
pub extern "C" fn Java_com_jeremy_stream_NativeBridge_sendVideoPacket(
    mut env: JNIEnv,
    _class: JClass,
    data_array: JByteArray,
    presentation_time_us: jlong,
    is_key_frame: jboolean,
) {
    let server_lock = match SERVER.read() {
        Ok(guard) => match &*guard {
            Some(s) => s,
            None => return,
        },
        Err(_) => return,
    };

    let server = match server_lock.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let bytes = match env.convert_byte_array(&data_array) {
        Ok(b) => b,
        Err(_) => return,
    };

    if bytes.is_empty() {
        return;
    }

    let mut packet = Vec::with_capacity(9 + bytes.len());
    packet.extend_from_slice(&presentation_time_us.to_le_bytes());
    packet.push(if is_key_frame == JNI_TRUE { 1 } else { 0 });
    packet.extend_from_slice(&bytes);

    let _ = server.socket.send_to(&packet, &server.target_addr);
}

/// Teardown handler when stopping the server
#[no_mangle]
pub extern "C" fn Java_com_jeremy_stream_NativeBridge_stopServer(
    _env: JNIEnv,
    _class: JClass,
) {
    if let Ok(mut guard) = SERVER.write() {
        *guard = None; // Drops the socket and frees the port immediately
    }
    println!("Rust streaming core shutdown and socket released.");
}
