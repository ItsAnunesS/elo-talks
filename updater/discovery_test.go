package main

import (
	"io"
	"net/http"
	"strings"
	"testing"
)

func TestStableDiscoveryUsesPublicRedirectWithoutREST(t *testing.T) {
	original := client
	defer func() { client = original }()
	for _, tc := range []struct {
		current, tag string
		newer        bool
	}{
		{"0.4.1", "v0.5.0", true},
		{"0.5.0", "v0.5.0", false},
		{"0.5.1", "v0.5.0", false},
	} {
		calls := 0
		client = &http.Client{CheckRedirect: original.CheckRedirect, Transport: releaseTransport(func(req *http.Request) (*http.Response, error) {
			calls++
			if req.Method != http.MethodHead || req.URL.Host != "github.com" {
				t.Fatalf("Stable checks must not use the REST API or fetch page bodies: %s %s", req.Method, req.URL)
			}
			if req.URL.Path == "/ItsAnunesS/elo-talks/releases/latest" {
				return &http.Response{StatusCode: 302, Header: http.Header{"Location": {repository + "/releases/tag/" + tc.tag}}, Body: io.NopCloser(strings.NewReader(""))}, nil
			}
			return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(""))}, nil
		})}
		r, err := latest(tc.current)
		if err != nil || (r != nil) != tc.newer || calls != 2 {
			t.Fatalf("unexpected discovery: %+v %v (%d requests)", r, err, calls)
		}
		if r != nil {
			if r.Tag != tc.tag {
				t.Fatal(r.Tag)
			}
			for _, name := range []string{imageName(tc.tag), "SHA256SUMS", "SHA256SUMS.sigstore.json"} {
				a, err := asset(r, name)
				if err != nil || !strings.HasPrefix(a.URL, repository+"/releases/download/"+tc.tag+"/") {
					t.Fatalf("untrusted generated asset: %+v %v", a, err)
				}
			}
		}
	}
}

func TestStableDiscoveryRejectsUntrustedOrNonStableLocations(t *testing.T) {
	original := client
	defer func() { client = original }()
	for _, location := range []string{
		"https://example.invalid/releases/tag/v0.5.0",
		"http://github.com/ItsAnunesS/elo-talks/releases/tag/v0.5.0",
		"https://github.com/another/repo/releases/tag/v0.5.0",
		repository + "/releases/tag/v0.5.0-rc.1",
		repository + "/releases/tag/v0.5.0/extra",
		repository + "/releases/tag/v0.5.0?unexpected=1",
	} {
		client = &http.Client{CheckRedirect: original.CheckRedirect, Transport: releaseTransport(func(req *http.Request) (*http.Response, error) {
			if req.URL.Path == "/ItsAnunesS/elo-talks/releases/latest" {
				return &http.Response{StatusCode: 302, Header: http.Header{"Location": {location}}, Body: io.NopCloser(strings.NewReader(""))}, nil
			}
			return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(""))}, nil
		})}
		if _, err := latest("0.4.1"); err == nil {
			t.Fatal("accepted", location)
		}
	}
}

func TestDiscoveryFailuresAndImageBoundsNeverReportCurrent(t *testing.T) {
	original := client
	defer func() { client = original }()
	client = &http.Client{Transport: releaseTransport(func(req *http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: 429, Header: http.Header{"Retry-After": {"120"}}, Body: io.NopCloser(strings.NewReader("private response"))}, nil
	})}
	_, err := latest("0.4.1")
	result := failureResult(err)
	if err == nil || result.ErrorCode != "rate_limited" || result.RetryAfter != 120 || strings.Contains(result.Error, "private") {
		t.Fatalf("lost safe failure details: %+v", result)
	}
	for _, size := range []int64{-1, 0, maxImage + 1, 1024} {
		client = &http.Client{Transport: releaseTransport(func(req *http.Request) (*http.Response, error) {
			if req.Method != http.MethodHead {
				t.Fatal(req.Method)
			}
			return &http.Response{StatusCode: 200, ContentLength: size, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(""))}, nil
		})}
		got, err := imageSize(Asset{URL: repository + "/releases/download/v0.5.0/" + imageName("v0.5.0")})
		if size == 1024 {
			if err != nil || got != size {
				t.Fatal(got, err)
			}
		} else if err == nil {
			t.Fatal("accepted invalid size", size)
		}
	}
}
