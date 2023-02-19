import urllib.request
import threading
import time
import random

def read_something():
    for _ in range(10000):
        #time.sleep(float(random.randrange(1, 10)) / 10)
        with urllib.request.urlopen("http://127.0.0.1:7878/hello.html") as request:
            _ = request.read()

def main():
    start_time = time.time()
    threads = [threading.Thread(target=read_something) for _ in range(10)]

    for thread in threads:
        thread.start()

    for thread in threads:
        thread.join()

    finish_time = time.time()
    print(f'Duration: {finish_time - start_time}')

if __name__ == '__main__':
    main()
