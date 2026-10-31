package main

import (
	"io"
	"net/http"
	"strings"
	"testing"
)

type releaseTransport func(*http.Request) (*http.Response, error)

func (f releaseTransport) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }

func TestReleaseChannels(t *testing.T) {
	original := client
	defer func() { client = original }()
	for _, tc := range []struct{ installed, body, expected, endpoint string }{
		{"0.1.30", `{"tag_name":"v0.2.0-rc.31","prerelease":true}`, "", "/releases/latest"},
		{"0.1.30", `{"tag_name":"v0.2.0","prerelease":false}`, "v0.2.0", "/releases/latest"},
		{"0.2.0-rc.31", `[{"tag_name":"v0.2.0-rc.33","prerelease":true},{"tag_name":"v0.2.0-rc.32","prerelease":true}]`, "v0.2.0-rc.33", "/releases?per_page=30"},
		{"0.2.0-rc.31", `[{"tag_name":"v0.3.0-rc.1","prerelease":true},{"tag_name":"v0.2.0","prerelease":false}]`, "v0.2.0", "/releases?per_page=30"},
		{"0.2.0-rc.31", `[{"tag_name":"v0.2.0-rc.32","prerelease":true,"draft":true}]`, "", "/releases?per_page=30"},
	} {
		t.Run(tc.installed+tc.expected, func(t *testing.T) {
			client = &http.Client{Transport: releaseTransport(func(r *http.Request) (*http.Response, error) {
				if !strings.HasSuffix(r.URL.String(), tc.endpoint) {
					t.Fatal(r.URL)
				}
				return &http.Response{StatusCode: 200, Body: io.NopCloser(strings.NewReader(tc.body)), Header: make(http.Header)}, nil
			})}
			release, err := latest(tc.installed)
			if err != nil {
				t.Fatal(err)
			}
			got := ""
			if release != nil {
				got = release.Tag
			}
			if got != tc.expected {
				t.Fatalf("got %q, want %q", got, tc.expected)
			}
		})
	}
}
