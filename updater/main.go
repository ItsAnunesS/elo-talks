// The updater only handles public release artifacts. It never opens vault data.
package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/sigstore/sigstore-go/pkg/bundle"
	"github.com/sigstore/sigstore-go/pkg/root"
	"github.com/sigstore/sigstore-go/pkg/verify"
)

const repository = "https://github.com/ItsAnunesS/elo-talks"
const identity = repository + "/.github/workflows/release.yml@refs/heads/release"
const maxImage = 512 * 1024 * 1024

var client = &http.Client{Timeout: 90 * time.Second, CheckRedirect: func(req *http.Request, via []*http.Request) error {
	if len(via) >= 5 || !downloadHost(req.URL) {
		return errors.New("untrusted redirect")
	}
	return nil
}}

type Asset struct {
	Name string `json:"name"`
	URL  string `json:"browser_download_url"`
	Size int64  `json:"size"`
}
type Release struct {
	Tag        string  `json:"tag_name"`
	Draft      bool    `json:"draft"`
	Prerelease bool    `json:"prerelease"`
	Assets     []Asset `json:"assets"`
}
type Result struct {
	RetryAfter uint64 `json:"retryAfter,omitempty"`
	Phase      string `json:"phase"`
	Version    string `json:"version,omitempty"`
	Stage      string `json:"stage,omitempty"`
	Error      string `json:"error,omitempty"`
}
type Staged struct {
	Version string `json:"version"`
	Asset   string `json:"asset"`
}

func version(s string) ([3]uint64, error) {
	var v [3]uint64
	parts := strings.Split(strings.TrimPrefix(s, "v"), ".")
	if len(parts) != 3 {
		return v, errors.New("invalid version")
	}
	for i, p := range parts {
		if p == "" || (len(p) > 1 && p[0] == '0') {
			return v, errors.New("invalid version")
		}
		for _, c := range p {
			if c < '0' || c > '9' {
				return v, errors.New("invalid version")
			}
		}
		n, e := strconv.ParseUint(p, 10, 32)
		if e != nil {
			return v, e
		}
		v[i] = n
	}
	return v, nil
}
func newer(candidate, current string) bool {
	a, e := version(candidate)
	b, f := version(current)
	if e != nil || f != nil {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return a[i] > b[i]
		}
	}
	return false
}
func downloadHost(u *url.URL) bool {
	if u.Scheme != "https" || u.User != nil || u.Port() != "" {
		return false
	}
	switch u.Hostname() {
	case "api.github.com", "github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com", "github-releases.githubusercontent.com":
		return true
	}
	return false
}
func get(address string, limit int64) ([]byte, error) {
	u, e := url.Parse(address)
	if e != nil || !downloadHost(u) {
		return nil, errors.New("untrusted URL")
	}
	req, e := http.NewRequest("GET", address, nil)
	if e != nil {
		return nil, e
	}
	req.Header.Set("User-Agent", "Elo-Talks-Updater")
	req.Header.Set("Accept", "application/vnd.github+json")
	response, e := client.Do(req)
	if e != nil {
		return nil, e
	}
	defer response.Body.Close()
	if response.StatusCode != 200 {
		return nil, responseError(response, time.Now())
	}
	data, e := io.ReadAll(io.LimitReader(response.Body, limit+1))
	if e != nil {
		return nil, e
	}
	if int64(len(data)) > limit {
		return nil, errors.New("response too large")
	}
	return data, nil
}
func latest(current string) (*Release, error) {
	data, e := get("https://api.github.com/repos/ItsAnunesS/elo-talks/releases/latest", 1024*1024)
	if e != nil {
		return nil, e
	}
	var release Release
	if e = json.Unmarshal(data, &release); e != nil {
		return nil, e
	}
	if release.Draft || release.Prerelease {
		return nil, errors.New("unsupported release")
	}
	if _, e = version(release.Tag); e != nil {
		return nil, e
	}
	if !newer(release.Tag, current) {
		return nil, nil
	}
	return &release, nil
}
func asset(r *Release, name string) (Asset, error) {
	var found *Asset
	for _, a := range r.Assets {
		if a.Name == name {
			if found != nil {
				return Asset{}, errors.New("duplicate asset")
			}
			copy := a
			found = &copy
		}
	}
	if found == nil {
		return Asset{}, errors.New("missing asset")
	}
	u, e := url.Parse(found.URL)
	if e != nil || !downloadHost(u) || u.Host != "github.com" || !strings.HasPrefix(u.Path, "/ItsAnunesS/elo-talks/releases/download/"+r.Tag+"/") {
		return Asset{}, errors.New("untrusted asset")
	}
	return *found, nil
}
func imageName(v string) string { return "Elo.Talks_" + strings.TrimPrefix(v, "v") + "_amd64.AppImage" }
func digest(manifest []byte, name string) (string, error) {
	result := ""
	for _, line := range strings.Split(string(manifest), "\n") {
		hash, file, ok := strings.Cut(line, "  ")
		if !ok {
			continue
		}
		if strings.ReplaceAll(file, " ", ".") != name {
			continue
		}
		if result != "" {
			return "", errors.New("duplicate checksum")
		}
		b, e := hex.DecodeString(hash)
		if e != nil || len(b) != 32 {
			return "", errors.New("invalid checksum")
		}
		result = strings.ToLower(hash)
	}
	if result == "" {
		return "", errors.New("checksum missing")
	}
	return result, nil
}
func authenticate(dir string) error {
	b, e := bundle.LoadJSONFromPath(filepath.Join(dir, "SHA256SUMS.sigstore.json"))
	if e != nil {
		return e
	}
	trusted, e := root.FetchTrustedRoot()
	if e != nil {
		return e
	}
	verifier, e := verify.NewVerifier(trusted, verify.WithSignedCertificateTimestamps(1), verify.WithTransparencyLog(1), verify.WithObserverTimestamps(1))
	if e != nil {
		return e
	}
	cert, e := verify.NewShortCertificateIdentity("https://token.actions.githubusercontent.com", "", identity, "")
	if e != nil {
		return e
	}
	manifest, e := os.ReadFile(filepath.Join(dir, "SHA256SUMS"))
	if e != nil {
		return e
	}
	_, e = verifier.Verify(b, verify.NewPolicy(verify.WithArtifact(bytes.NewReader(manifest)), verify.WithCertificateIdentity(cert)))
	return e
}
func regular(path string) error {
	s, e := os.Lstat(path)
	if e != nil {
		return e
	}
	if !s.Mode().IsRegular() {
		return errors.New("not a regular file")
	}
	return nil
}
func imageHash(path, want string) error {
	if e := regular(path); e != nil {
		return e
	}
	f, e := os.Open(path)
	if e != nil {
		return e
	}
	defer f.Close()
	h := sha256.New()
	n, e := io.Copy(h, io.LimitReader(f, maxImage+1))
	if e != nil {
		return e
	}
	if n > maxImage || hex.EncodeToString(h.Sum(nil)) != want {
		return errors.New("image checksum mismatch")
	}
	return nil
}
func prepare(current, target, expected string) (Result, error) {
	if !filepath.IsAbs(target) {
		return Result{}, errors.New("invalid target")
	}
	if e := regular(target); e != nil {
		return Result{}, e
	}
	r, e := latest(current)
	if e != nil {
		return Result{}, e
	}
	if r == nil || strings.TrimPrefix(r.Tag, "v") != expected {
		return Result{}, errors.New("release changed; check again")
	}
	name := imageName(r.Tag)
	a, e := asset(r, name)
	if e != nil {
		return Result{}, e
	}
	if a.Size <= 0 || a.Size > maxImage {
		return Result{}, errors.New("invalid image size")
	}
	dir, e := os.MkdirTemp(filepath.Dir(target), ".elo-update-")
	if e != nil {
		return Result{}, e
	}
	success := false
	defer func() {
		if !success {
			os.RemoveAll(dir)
		}
	}()
	for _, name := range []string{"SHA256SUMS", "SHA256SUMS.sigstore.json"} {
		a, e := asset(r, name)
		if e != nil {
			return Result{}, e
		}
		data, e := get(a.URL, 1024*1024)
		if e != nil {
			return Result{}, e
		}
		if e = os.WriteFile(filepath.Join(dir, name), data, 0600); e != nil {
			return Result{}, e
		}
	}
	if e = authenticate(dir); e != nil {
		return Result{}, errors.New("release signature could not be verified")
	}
	manifest, _ := os.ReadFile(filepath.Join(dir, "SHA256SUMS"))
	want, e := digest(manifest, name)
	if e != nil {
		return Result{}, e
	}
	// Stream the AppImage to disk; never buffer hundreds of megabytes in memory.
	req, _ := http.NewRequest("GET", a.URL, nil)
	req.Header.Set("User-Agent", "Elo-Talks-Updater")
	response, e := client.Do(req)
	if e != nil {
		return Result{}, e
	}
	defer response.Body.Close()
	if response.StatusCode != 200 {
		return Result{}, responseError(response, time.Now())
	}
	path := filepath.Join(dir, "new.AppImage")
	f, e := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0600)
	if e != nil {
		return Result{}, e
	}
	n, copyErr := io.Copy(f, io.LimitReader(response.Body, maxImage+1))
	syncErr := f.Sync()
	closeErr := f.Close()
	if copyErr != nil || syncErr != nil || closeErr != nil || n != a.Size {
		return Result{}, errors.New("download incomplete or disk full")
	}
	if e = imageHash(path, want); e != nil {
		return Result{}, e
	}
	data, _ := json.Marshal(Staged{expected, name})
	if e = os.WriteFile(filepath.Join(dir, "release.json"), data, 0600); e != nil {
		return Result{}, e
	}
	success = true
	return Result{Phase: "ready", Version: expected, Stage: dir}, nil
}
func install(current, target, dir string) error {
	if !filepath.IsAbs(target) || !filepath.IsAbs(dir) || filepath.Dir(dir) != filepath.Dir(target) || !strings.HasPrefix(filepath.Base(dir), ".elo-update-") {
		return errors.New("invalid staging directory")
	}
	info, e := os.Lstat(dir)
	if e != nil || !info.IsDir() || info.Mode().Perm() != 0700 {
		return errors.New("untrusted staging directory")
	}
	if e = regular(target); e != nil {
		return e
	}
	for _, name := range []string{"release.json", "SHA256SUMS", "SHA256SUMS.sigstore.json", "new.AppImage"} {
		if e = regular(filepath.Join(dir, name)); e != nil {
			return e
		}
		s, _ := os.Stat(filepath.Join(dir, name))
		if name != "new.AppImage" && s.Size() > 1024*1024 {
			return errors.New("metadata too large")
		}
	}
	data, e := os.ReadFile(filepath.Join(dir, "release.json"))
	if e != nil {
		return e
	}
	var staged Staged
	if e = json.Unmarshal(data, &staged); e != nil {
		return e
	}
	if !newer(staged.Version, current) || staged.Asset != imageName(staged.Version) {
		return errors.New("invalid staged version")
	}
	if e = authenticate(dir); e != nil {
		return errors.New("release signature could not be verified")
	}
	manifest, _ := os.ReadFile(filepath.Join(dir, "SHA256SUMS"))
	want, e := digest(manifest, staged.Asset)
	if e != nil {
		return e
	}
	image := filepath.Join(dir, "new.AppImage")
	if e = imageHash(image, want); e != nil {
		return e
	}
	return replaceImage(image, target, dir)
}

func replaceImage(image, target, dir string) error {
	if e := os.Chmod(image, 0755); e != nil {
		return e
	}
	// Preserve the old inode, then atomically replace the launcher target.
	backup := filepath.Join(dir, "previous.AppImage")
	if e := os.Link(target, backup); e != nil {
		return e
	}
	if e := os.Rename(image, target); e != nil {
		_ = os.Remove(backup)
		return e
	}
	// Once rename succeeds the new, fully verified executable is installed.
	// A directory fsync failure must not misreport that the old image is installed.
	if parent, err := os.Open(filepath.Dir(target)); err == nil {
		_ = parent.Sync()
		_ = parent.Close()
	}
	return nil
}
func main() {
	result := Result{Phase: "error"}
	var err error
	args := os.Args[1:]
	if len(args) == 2 && args[0] == "verify" {
		err = authenticate(args[1])
		if err == nil {
			result.Phase = "verified"
		}
	} else if len(args) == 2 && args[0] == "check" {
		var r *Release
		r, err = latest(args[1])
		if err == nil {
			result.Phase = "current"
			if r != nil {
				result.Phase = "available"
				result.Version = strings.TrimPrefix(r.Tag, "v")
			}
		}
	} else if len(args) == 4 && args[0] == "prepare" {
		result, err = prepare(args[1], args[2], args[3])
	} else if len(args) == 4 && args[0] == "install" {
		err = install(args[1], args[2], args[3])
		if err == nil {
			result.Phase = "installed"
		}
	} else {
		err = errors.New("invalid operation")
	}
	if err != nil {
		result = failureResult(err)
	}
	_ = json.NewEncoder(os.Stdout).Encode(result)
	if err != nil {
		os.Exit(1)
	}
}
