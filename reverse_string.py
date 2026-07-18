# Reverse a String

def reverse_string(s):
    return s[::-1]

# Test the function
if __name__ == "__main__":
    test_str = "hello"
    print(f"Original: {test_str}")
    reversed = reverse_string(test_str)
    print(f"Reversed: {reversed}")