use std::io;
use std::net::TcpListener;

// Helper function to convert byte offset to (line, column) position
pub fn offset_to_position(content: &str, offset: usize) -> (u32, u32) {
    let mut line = 0;
    let mut line_start_offset = 0;

    // Find the line containing the offset by counting newlines
    for (i, c) in content.chars().take(offset).enumerate() {
        if c == '\n' {
            line += 1;
            line_start_offset = i + 1; // +1 to skip the newline character
        }
    }

    // Character offset within the line
    let character = (offset - line_start_offset) as u32;

    (line, character)
}

// Function to find an available port starting from the given port
pub fn find_available_port(start_port: u16) -> io::Result<u16> {
    let mut port = start_port;
    let max_attempts = 10; // Try up to 10 ports

    for _ in 0..max_attempts {
        match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(_) => return Ok(port),
            Err(_) => {
                // Port is not available, try the next one
                port += 1;
            }
        }
    }

    // If we've tried all ports and none are available, return an error
    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        format!(
            "Could not find an available port after trying {} ports",
            max_attempts
        ),
    ))
}
