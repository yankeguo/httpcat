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

var (
	optPort         string
	optResponseBody []byte
	optResponseType string
	optResponseCode int
)

func init() {
	optPort = os.Getenv("PORT")
	if optPort == "" {
		optPort = "80"
	}
	optResponseBody = []byte(os.Getenv("RESPONSE_BODY"))
	if len(optResponseBody) == 0 {
		optResponseBody = []byte("OK")
	}
	optResponseType = os.Getenv("RESPONSE_TYPE")
	if optResponseType == "" {
		optResponseType = "text/plain; charset=utf-8"
	}
	optResponseCode, _ = strconv.Atoi(os.Getenv("RESPONSE_CODE"))
	if optResponseCode == 0 {
		optResponseCode = http.StatusOK
	}
}

func main() {
	log.SetFlags(0)
	log.SetOutput(os.Stdout)

	count := uint64(0)
	locker := &sync.Mutex{}

	http.HandleFunc("/", func(rw http.ResponseWriter, req *http.Request) {
		locker.Lock()
		defer locker.Unlock()
		count++

		// request id
		title := fmt.Sprintf("================== %s ==== #%d ==================", time.Now().Format(time.RFC3339), count)
		log.Printf(title)
		// proto / method / url
		log.Println("")
		log.Printf("%s %s %s", req.Proto, req.Method, req.URL.String())
		// headers
		log.Println("")
		// fix for golang Host header
		log.Printf("Host: %s", req.Host)
		for k, vs := range req.Header {
			for _, v := range vs {
				log.Printf("%s: %s", k, v)
			}
		}
		// body
		log.Println("")
		br := bufio.NewReader(req.Body)
		for {
			line, err := br.ReadString('\n')
			log.Println(line)
			if err != nil {
				break
			}
		}
		endLine := &strings.Builder{}
		for range title {
			endLine.WriteRune('=')
		}
		log.Println(endLine.String())

		// response with OK
		rw.Header().Set("Content-Type", optResponseType)
		rw.Header().Set("Content-Length", strconv.Itoa(len(optResponseBody)))
		rw.WriteHeader(optResponseCode)
		_, _ = rw.Write(optResponseBody)
	})

	log.Printf("listening at %s", optPort)
	log.Fatal(http.ListenAndServe(":"+optPort, nil))
}
