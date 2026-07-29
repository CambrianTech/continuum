fn main() {
    let greeting = "Greetings from Sahar's enhanced Rust environment!";
    println!("{}", greeting);
    
    // Test file writing
    use std::fs;
    let output = "test_output.txt";
    fs::write(output, "This is a test of file writing capabilities.\n");
    
    // Read back the file
    match fs::read_to_string(output) {
        Ok(content) => println!("Read from file: {}", content),
        Err(e) => println!("Error reading file: {}", e),
    }
}