package main

import (
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

type requestError struct {
	status     int
	retryAfter uint64
}

var errSignature = errors.New("release signature could not be verified")

func (e *requestError) Error() string { return fmt.Sprintf("HTTP %d", e.status) }

// Return a delay, never a server response body or a potentially sensitive URL.
func responseError(response *http.Response, now time.Time) error {
	var delay uint64
	raw := strings.TrimSpace(response.Header.Get("Retry-After"))
	if seconds, err := strconv.ParseUint(raw, 10, 63); err == nil {
		delay = seconds
	} else if deadline, err := http.ParseTime(raw); err == nil && deadline.After(now) {
		delay = uint64(deadline.Unix()-now.Unix()) + 1
	}
	if response.Header.Get("X-RateLimit-Remaining") == "0" {
		if reset, err := strconv.ParseInt(response.Header.Get("X-RateLimit-Reset"), 10, 64); err == nil && reset > now.Unix() {
			seconds := uint64(reset-now.Unix()) + 1
			if seconds > delay {
				delay = seconds
			}
		}
	}
	return &requestError{status: response.StatusCode, retryAfter: delay}
}
func failureResult(err error) Result {
	result := Result{Phase: "error", ErrorCode: "verification_failed", Error: "The update could not be verified. Please try again later."}
	var remote *requestError
	if errors.As(err, &remote) {
		result.RetryAfter = remote.retryAfter
		switch {
		case remote.status == 429 || remote.status == 403 && remote.retryAfter > 0:
			result.ErrorCode = "rate_limited"
			result.Error = "GitHub has temporarily limited update requests. Please wait before trying again."
		case remote.status >= 500:
			result.ErrorCode = "service_unavailable"
			result.Error = "The update service is temporarily unavailable. Please try again later."
		default:
			result.ErrorCode = "release_unavailable"
			result.Error = "The release could not be retrieved. Please try again later."
		}
	} else if errors.Is(err, errSignature) {
		result.ErrorCode = "signature_invalid"
		result.Error = "The update signature could not be verified. Installation was stopped."
	} else {
		var network *url.Error
		var timeout net.Error
		if errors.As(err, &network) {
			result.ErrorCode = "connection_failed"
			result.Error = "Could not connect to the update service. Check your internet connection."
		}
		if errors.As(err, &timeout) && timeout.Timeout() {
			result.ErrorCode = "network_timeout"
			result.Error = "The update service took too long to respond. Please try again."
		}
	}
	return result
}
