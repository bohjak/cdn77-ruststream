package main

import (
	"fmt"
	"net"
	"time"
    "sync"
)

func send(path string) error {
    var wg sync.WaitGroup
    conn, err := net.Dial("tcp", "localhost:3000")
    if err != nil {
        return err
    }

    headers := fmt.Sprintf("PUT %s HTTP/1.1\r\n", path)
    headers += "Host: localhost:3000\r\n"
    headers += "Transfer-Encoding: chunked\r\n"
    headers += "Connection: close\r\n"
    headers += "\r\n"
    conn.Write([]byte(headers))

    ticker := time.NewTicker(1 * time.Second)

    counter := 0

    wg.Add(1)
    go func() {
        done := false
        for done != true {
            select {
                case <-ticker.C: {
                    msg := fmt.Sprintf("[%d] %s", counter, time.Now().Format("15:04:05"))
                    fmt.Println("Sent:", msg)
                    chunk := fmt.Sprintf("%x\r\n%s\r\n", len(msg), msg)
                    conn.Write([]byte(chunk))
                    counter += 1
                    if counter > 100 {
                        done = true
                    }
                }
            }
        }
        conn.Write([]byte("0\r\n\r\n"))
        wg.Done()
        ticker.Stop()
        conn.Close()
    }()
    wg.Wait()

    return nil
}

func read(path string) error {
    conn, err := net.Dial("tcp", "localhost:3000")
    defer conn.Close()
    if err != nil {
        return err
    }

    headers := fmt.Sprintf("GET %s HTTP/1.1\r\n", path)
    headers += "Host: localhost:3000\r\n"
    headers += "\r\n"
    conn.Write([]byte(headers))

    buffer := make([]byte, 1024)

    n, err := conn.Read(buffer)
    if err != nil {
        return err
    }

    fmt.Printf("Received: %s\n", buffer[:n])

    return nil
}

func readLoop(path string) {
    var wg sync.WaitGroup
    ticker := time.NewTicker(1 * time.Second)

    wg.Add(1)
    go func() {
        for {
            select {
                case <-ticker.C: {
                    read(path)
                }
            }
        }
    }()
    wg.Wait()
}

func readChunked(path string) error {
    conn, err := net.Dial("tcp", "localhost:3000")
    defer conn.Close()
    if err != nil {
        return err
    }

    headers := fmt.Sprintf("GET %s HTTP/1.1\r\n", path)
    headers += "Host: localhost:3000\r\n"
    headers += "Transfer-Encoding: chunked\r\n"
    headers += "\r\n"
    conn.Write([]byte(headers))

    buffer := make([]byte, 1024)

    for {
        n, err := conn.Read(buffer)
        if err != nil {
            return err
        }

        fmt.Printf("Received: %s\n", buffer[:n])
    }
}

func main() {
    var wg sync.WaitGroup
    path := "/test/concurrence"
    wg.Add(2)
    go func() {
        defer wg.Done()
        send(path)
    }()
    go func() {
        defer wg.Done()
        readLoop(path)
    }()
    // time.Sleep(7 * time.Second)
    // readChunked(path)
    wg.Wait()
}
