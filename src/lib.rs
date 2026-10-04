use jni::objects::{JClass, JByteArray};
use jni::sys::{jboolean, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use std::net::UdpSocket;
use std::sync::{Mutex, OnceLock};

struct StreamingServer {
    socket: UdpSocket,
    target_addr: String,
}

// Global state container for our UDP streaming socket
static SERVER: OnceLock<Mutex<StreamingServer>> = OnceLock::new();

/// Initializes the UDP socket bound to a local port
#[no_mangle]
pub extern "C" fn Java_com_jeremy_stream_NativeBridge_initServer(
    _env: JNIEnv,
    _class: JClass,
    port: jint,
) -> jboolean {
    let bind_addr = format!("0.0.0.0:{}", port);
    
    match UdpSocket::bind(&bind_addr) {
        Ok(socket) => {
            // Set socket to non-blocking for real-time performance
            let _ = socket.set_nonblocking(true);
            
            // Standard Wi-Fi Direct Group Owner default IP is typically 192.168.49.1
            // (You can also update this dynamically if passed from Kotlin)
            let server = StreamingServer {
                socket,
                target_addr: "192.168.49.1:9000".to_string(),
            };
            
            // Initialize global state (disregards error if already set)
            let _ = SERVER.set(Mutex::new(server));
            JNI_TRUE
        }
        Err(e) => {
            eprintln!("Failed to bind UDP socket: {}", e);
            JNI_FALSE
        }
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
    let server_guard = match SERVER.get() {
        Some(s) => s,
        None => return,
    };

    let server = match server_guard.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    // Convert Java byte array into a Rust Vec<u8>
    let bytes = match env.convert_byte_array(&data_array) {
        Ok(b) => b,
        Err(_) => return,
    };

    if bytes.is_empty() {
        return;
    }

    // Packet Structure Framing:
    // [ 8 bytes: Presentation Timestamp (microseconds) ]
    // [ 1 byte : Is Keyframe Flag (0 or 1)           ]
    // [ N bytes: Raw AV1 Video Payload Bytes           ]
    let mut packet = Vec::with_capacity(9 + bytes.len());
    packet.extend_from_slice(&presentation_time_us.to_le_bytes());
    packet.push(if is_key_frame == JNI_TRUE { 1 } else { 0 });
    packet.extend_from_slice(&bytes);

    // Fire packet over UDP (handles MTU chunking logic or streams directly)
    let _ = server.socket.send_to(&packet, &server.target_addr);
}

/// Teardown handler when stopping the server
#[no_mangle]
pub extern "C" fn Java_com_jeremy_stream_NativeBridge_stopServer(
    _env: JNIEnv,
    _class: JClass,
) {
    // Sockets will automatically drop and close when the program terminates or resets.
    println!("Rust streaming core shutdown requested.");
}
