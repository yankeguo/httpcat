FROM golang:1.19 AS builder
ENV CGO_ENABLED 0
WORKDIR /go/src/app
ADD . .
RUN go build -o /httpcat

FROM scratch
COPY --from=builder /httpcat /httpcat
CMD ["/httpcat"]
