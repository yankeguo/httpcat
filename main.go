package main

import (
	"bufio"
	"fmt"
	"log"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

type Options struct {
	Port         string
	ResponseBody []byte
	ResponseType string
	ResponseCode int
}

func envStr(key string) string {
	return strings.TrimSpace(os.Getenv(key))
}

func envInt(key string) int {
	v, _ := strconv.Atoi(envStr(key))
	return v
}

func main() {
	log.SetFlags(0)
	log.SetOutput(os.Stdout)

	opts := Options{
		Port:         envStr("PORT"),
		ResponseBody: []byte(envStr("RESPONSE_BODY")),
		ResponseType: envStr("RESPONSE_TYPE"),
		ResponseCode: envInt("RESPONSE_CODE"),
	}

	if opts.Port == "" {
		opts.Port = "80"
	}
	if len(opts.ResponseBody) == 0 {
		opts.ResponseBody = []byte("OK")
	}
	if opts.ResponseType == "" {
		opts.ResponseType = "text/plain; charset=utf-8"
	}
	if opts.ResponseCode == 0 {
		opts.ResponseCode = http.StatusOK
	}

	var (
		id   = uint64(0)
		lock = &sync.Mutex{}
	)

	http.HandleFunc("/", func(rw http.ResponseWriter, req *http.Request) {
		lock.Lock()
		defer lock.Unlock()

		// increase id
		id++

		// print start line
		log.Println()
		startLine := fmt.Sprintf(
			"================== %s ==== #%d ==================",
			time.Now().Format(time.RFC3339),
			id,
		)
		log.Printf(startLine)

		// print stop line
		stopLine := make([]byte, len(startLine), len(startLine))
		for i := range stopLine {
			stopLine[i] = '='
		}
		defer log.Println(string(stopLine))

		// print proto / method / url
		log.Println()
		log.Printf("%s %s %s", req.Proto, req.Method, req.URL.String())
		// print headers
		log.Println()
		log.Printf("Host: %s", req.Host)
		for k, vs := range req.Header {
			for _, v := range vs {
				log.Printf("%s: %s", k, v)
			}
		}

		// print body
		log.Println()
		br := bufio.NewReader(req.Body)
		for {
			line, err := br.ReadString('\n')
			log.Println(line)
			if err != nil {
				break
			}
		}

		// response
		rw.Header().Set("Content-Type", opts.ResponseType)
		rw.Header().Set("Content-Length", strconv.Itoa(len(opts.ResponseBody)))
		rw.WriteHeader(opts.ResponseCode)
		_, _ = rw.Write(opts.ResponseBody)
	})

	log.Println("Listening at", opts.Port)
	log.Fatal(http.ListenAndServe(":"+opts.Port, nil))
}
