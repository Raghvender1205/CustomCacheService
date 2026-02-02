import socket
import struct

def send_command(host, port, command_type, **kwargs):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        try:
            sock.connect((host, port))
            message = bytearray([command_type])

            for value in kwargs.values():
                if isinstance(value, str):
                    message.extend(struct.pack('B', len(value)))
                    message.extend(value.encode('utf-8'))
                elif isinstance(value, int):
                    message.extend(struct.pack('<Q', value))
                else:
                    raise ValueError(f"Unsupported type for value: {type(value)}")
            
            # Send the message
            sock.sendall(message)

            # Receive response (up to 1024 bytes)
            response = sock.recv(1024).decode('utf-8')
            print(f"Received: {response}")
        except ConnectionRefusedError:
            print("Connection refused. Make sure the server is running.")
        except Exception as e:
            print(f"An error occurred: {e}")

# Example usage
if __name__ == "__main__":
    host = '127.0.0.1'
    port = 6379

    # Send a SET command
    send_command(host, port, 0x01, key="hello", value="world")

    # Set an expiration on 'hello'
    send_command(host, port, 0x04, key="hello", seconds=10)

    # Send a GET command
    send_command(host, port, 0x02, key="hello")

    # Increment a numerical value
    send_command(host, port, 0x01, key="counter", value="10")
    send_command(host, port, 0x05, key="counter")

    # Decrement the same numerical value
    send_command(host, port, 0x06, key="counter")

    # Get all keys (assuming there are some keys set in the cache)
    send_command(host, port, 0x07, pattern="*")

    # Send a DELETE command
    send_command(host, port, 0x03, key="hello")
