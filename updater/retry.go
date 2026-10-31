package main

import (
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"
)

type requestError struct {
	status     int
	retryAfter uint64
}

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
	result := Result{Phase: "error", Error: "Update could not be completed. Your installed version was preserved unless installation had already completed."}
	var remote *requestError
	if errors.As(err, &remote) {
		result.RetryAfter = remote.retryAfter
	}
	return result
}
