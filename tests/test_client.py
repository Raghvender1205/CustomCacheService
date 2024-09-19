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

    # Set an expiration on 'hello'
    send_command(host, port, {"Expire": {"key": "hello", "seconds": 10}})

    # Send a GET command
    send_command(host, port, {"Get": {"key": "hello"}})

    # Increment a numerical value
    send_command(host, port, {"Set": {"key": "counter", "value": "10"}})
    send_command(host, port, {"Incr": {"key": "counter"}})

    # Decrement the same numerical value
    send_command(host, port, {"Decr": {"key": "counter"}})

    # Get all keys (assuming there are some keys set in the cache)
    send_command(host, port, {"Keys": {"pattern": "*"}})

    # Send a DELETE command
    send_command(host, port, {"Delete": {"key": "hello"}})
