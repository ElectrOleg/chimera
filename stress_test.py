import socket
import time
import threading
import sys

TARGET_HOST = "127.0.0.1"
TARGET_PORT = 8080

def test_zombie_connection():
    """Connects but sends nothing, holding the socket open."""
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((TARGET_HOST, TARGET_PORT))
        print("[Zombie] Connected. Sleeping...")
        time.sleep(7) # longer than 5s timeout
        data = s.recv(1024)
        if not data:
            print("[Zombie] Connection closed by server (Success - Timeout worked)")
        else:
            print("[Zombie] Received data unexpectedly!")
        s.close()
    except Exception as e:
        print(f"[Zombie] Error (Expected if server closed): {e}")

def test_invalid_handshake():
    """Sends garbage data."""
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((TARGET_HOST, TARGET_PORT))
        s.send(b"GET /garbage HTTP/1.1\r\n\r\n")
        data = s.recv(1024)
        if not data:
            print("[Invalid] Connection closed by server (Success)")
        else:
            print(f"[Invalid] Server replied: {data}")
        s.close()
    except Exception as e:
        print(f"[Invalid] Error: {e}")

def stress_flood(count=50):
    """Rapidly connects and disconnects."""
    print(f"[Flood] Starting {count} connections...")
    def task():
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.connect((TARGET_HOST, TARGET_PORT))
            s.close()
        except:
            pass
            
    threads = []
    for _ in range(count):
        t = threading.Thread(target=task)
        t.start()
        threads.append(t)
    
    for t in threads:
        t.join()
    print("[Flood] Completed.")

if __name__ == "__main__":
    print("--- Starting Chimera Stress Tests ---")
    
    print("\n1. Testing Zombie Connection (Expect Timeout > 5s)")
    test_zombie_connection()
    
    print("\n2. Testing Invalid Handshake (Expect Immediate Close)")
    test_invalid_handshake()
    
    print("\n3. Testing Connection Flood")
    stress_flood(100)
    
    print("\nTests Finished.")
