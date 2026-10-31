package main

import (
	"fmt"
	"net/http"
	"testing"
	"time"
)

func TestRetryHeaders(t *testing.T) {
	now := time.Unix(1800000000, 0)
	for _, tc := range []struct {
		name, retry, remaining, reset string
		want                          uint64
	}{
		{"seconds", "120", "", "", 120},
		{"date", now.Add(10 * time.Minute).UTC().Format(http.TimeFormat), "", "", 601},
		{"exhausted", "30", "0", "1800000900", 901},
		{"later retry", "1200", "0", "1800000900", 1200},
		{"not exhausted", "", "4", "1800000900", 0},
		{"past", "", "0", "1799999999", 0},
		{"malformed", "-1", "0", "invalid", 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			response := &http.Response{StatusCode: 429, Header: make(http.Header)}
			response.Header.Set("Retry-After", tc.retry)
			response.Header.Set("X-RateLimit-Remaining", tc.remaining)
			response.Header.Set("X-RateLimit-Reset", tc.reset)
			result := failureResult(fmt.Errorf("wrapped: %w", responseError(response, now)))
			if result.RetryAfter != tc.want || result.Phase != "error" {
				t.Fatalf("unexpected result: %+v", result)
			}
		})
	}
}

func TestCandidateOrdering(t *testing.T) {
	for _, pair := range [][2]string{{"0.2.0-rc.2", "0.2.0-rc.1"}, {"0.2.0", "0.2.0-rc.99"}, {"0.2.0-rc.1", "0.1.30"}} {
		if !newer(pair[0], pair[1]) || newer(pair[1], pair[0]) {
			t.Fatal(pair)
		}
	}
	for _, invalid := range []string{"0.2.0-rc.0", "0.2.0-rc.01", "0.2.0-rc.-1", "0.2.0-beta.1", "0.2.0-rc.1/asset"} {
		if _, err := version(invalid); err == nil {
			t.Fatal(invalid)
		}
	}
}
