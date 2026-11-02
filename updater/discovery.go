package main

import (
	"errors"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Stable discovery uses GitHub's public latest-release redirect, not the shared
// anonymous REST quota. It is a discovery hint: signed checksums still authorize
// every downloaded image before staging and again before installation.
func latestStable(current string) (*Release, error) {
	req, _ := http.NewRequest(http.MethodHead, repository+"/releases/latest", nil)
	req.Header.Set("User-Agent", "Elo-Talks-Updater")
	response, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return nil, responseError(response, time.Now())
	}
	if response.Request == nil || response.Request.URL == nil {
		return nil, errors.New("missing release location")
	}
	final := response.Request.URL
	prefix := "/ItsAnunesS/elo-talks/releases/tag/"
	if !downloadHost(final) || final.Host != "github.com" || final.RawQuery != "" || final.Fragment != "" || !strings.HasPrefix(final.Path, prefix) {
		return nil, errors.New("untrusted release location")
	}
	tag := strings.TrimPrefix(final.Path, prefix)
	parsed, err := version(tag)
	if err != nil || !strings.HasPrefix(tag, "v") || parsed[3] != ^uint64(0) {
		return nil, errors.New("invalid stable release location")
	}
	if !newer(tag, current) {
		return nil, nil
	}
	release := &Release{Tag: tag}
	for _, name := range []string{imageName(tag), "SHA256SUMS", "SHA256SUMS.sigstore.json"} {
		release.Assets = append(release.Assets, Asset{Name: name, URL: repository + "/releases/download/" + tag + "/" + url.PathEscape(name)})
	}
	return release, nil
}

func imageSize(asset Asset) (int64, error) {
	if asset.Size != 0 {
		if asset.Size < 0 || asset.Size > maxImage {
			return 0, errors.New("invalid image size")
		}
		return asset.Size, nil
	}
	req, _ := http.NewRequest(http.MethodHead, asset.URL, nil)
	req.Header.Set("User-Agent", "Elo-Talks-Updater")
	response, err := client.Do(req)
	if err != nil {
		return 0, err
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return 0, responseError(response, time.Now())
	}
	if response.ContentLength <= 0 || response.ContentLength > maxImage {
		return 0, errors.New("invalid image size")
	}
	return response.ContentLength, nil
}
