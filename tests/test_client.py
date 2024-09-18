import socket
import json

def send_command(host, port, command):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.connect((host, port))
        message = json.dumps(command).encode('utf-8')
        sock.sendall(message)

        # Waiting for response
        response = sock.recv(1024)
        print("Received:", response.decode('utf-8'))

# Example usage
if __name__ == "__main__":
    host = '127.0.0.1'
    port = 6379

    # Send a SET command
    send_command(host, port, {"Set": {"key": "hello", "value": "world"}})

    # Send a GET command
    send_command(host, port, {"Get": {"key": "hello"}})

    # Send a DELETE command
    send_command(host, port, {"Delete": {"key": "hello"}})
