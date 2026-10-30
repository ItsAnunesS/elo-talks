package main

import (
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestVersionOrder(t *testing.T) {
	for _, pair := range [][2]string{{"0.1.20", "0.1.9"}, {"v1.0.0", "0.99.99"}} {
		if !newer(pair[0], pair[1]) {
			t.Fatal(pair)
		}
	}
	for _, candidate := range []string{"0.1.9", "0.1.8", "0.1.10-beta", "0.01.11", "../0.1.12", "0.1.+12"} {
		if newer(candidate, "0.1.9") {
			t.Fatal(candidate)
		}
	}
}
func TestReleaseAssetOrigin(t *testing.T) {
	for _, address := range []string{"http://github.com/a", "https://github.com.evil.test/a", "https://user@github.com/a", "https://github.com:8443/a"} {
		u, _ := url.Parse(address)
		if downloadHost(u) {
			t.Fatal(address)
		}
	}
	r := Release{Tag: "v0.1.20", Assets: []Asset{{Name: "image", URL: repository + "/releases/download/v0.1.19/image"}}}
	if _, err := asset(&r, "image"); err == nil {
		t.Fatal("accepted another release")
	}
}
func TestManifestBinding(t *testing.T) {
	hash := strings.Repeat("a", 64)
	line := hash + "  Elo Talks_0.1.20_amd64.AppImage\n"
	if got, err := digest([]byte(line), imageName("v0.1.20")); err != nil || got != hash {
		t.Fatal(got, err)
	}
	for _, data := range []string{line + line, "bad  Elo Talks_0.1.20_amd64.AppImage", line} {
		name := imageName("0.1.20")
		if data == line {
			name = imageName("0.1.21")
		}
		if _, err := digest([]byte(data), name); err == nil {
			t.Fatal("accepted invalid manifest")
		}
	}
}
func TestRejectTamperingAndSymlinks(t *testing.T) {
	dir := t.TempDir()
	image := filepath.Join(dir, "image")
	os.WriteFile(image, []byte("tampered"), 0600)
	if imageHash(image, strings.Repeat("0", 64)) == nil {
		t.Fatal("accepted tampering")
	}
	link := filepath.Join(dir, "link")
	os.Symlink(image, link)
	if regular(link) == nil {
		t.Fatal("accepted symlink")
	}
	if authenticate(dir) == nil {
		t.Fatal("accepted unsigned update")
	}
	if install("0.1.1", image, dir) == nil {
		t.Fatal("accepted untrusted staging")
	}
}

func TestAtomicReplacementPreservesPreviousImage(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "Elo Talks.AppImage")
	stage := filepath.Join(dir, "stage")
	os.Mkdir(stage, 0700)
	image := filepath.Join(stage, "new.AppImage")
	os.WriteFile(target, []byte("old verified image"), 0755)
	os.WriteFile(image, []byte("new verified image"), 0600)
	if err := replaceImage(image, target, stage); err != nil {
		t.Fatal(err)
	}
	current, _ := os.ReadFile(target)
	previous, _ := os.ReadFile(filepath.Join(stage, "previous.AppImage"))
	if string(current) != "new verified image" || string(previous) != "old verified image" {
		t.Fatal("replacement lost an image")
	}
	info, _ := os.Stat(target)
	if info.Mode().Perm() != 0755 {
		t.Fatal("image is not executable")
	}
}
func TestReplacementFailurePreservesTarget(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "current")
	image := filepath.Join(dir, "new")
	os.WriteFile(target, []byte("old"), 0755)
	os.WriteFile(image, []byte("new"), 0600)
	// Existing backup prevents the transaction from overwriting previous recovery data.
	os.WriteFile(filepath.Join(dir, "previous.AppImage"), []byte("backup"), 0600)
	if replaceImage(image, target, dir) == nil {
		t.Fatal("overwrote recovery data")
	}
	current, _ := os.ReadFile(target)
	if string(current) != "old" {
		t.Fatal("changed current image after failure")
	}
}

func TestUntrustedSignatureCannotReplaceInstalledImage(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "current.AppImage")
	os.WriteFile(target, []byte("installed"), 0755)
	stage, err := os.MkdirTemp(dir, ".elo-update-")
	if err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(stage, "release.json"), []byte(`{"version":"0.1.99","asset":"Elo.Talks_0.1.99_amd64.AppImage"}`), 0600)
	os.WriteFile(filepath.Join(stage, "new.AppImage"), []byte("attacker"), 0600)
	os.WriteFile(filepath.Join(stage, "SHA256SUMS"), []byte(strings.Repeat("0", 64)+"  Elo Talks_0.1.99_amd64.AppImage\n"), 0600)
	os.WriteFile(filepath.Join(stage, "SHA256SUMS.sigstore.json"), []byte(`{}`), 0600)
	if install("0.1.1", target, stage) == nil {
		t.Fatal("accepted untrusted update")
	}
	current, _ := os.ReadFile(target)
	if string(current) != "installed" {
		t.Fatal("signature failure changed executable")
	}
}

func TestPublishedReleaseSignature(t *testing.T) {
	if os.Getenv("ELO_UPDATER_NETWORK_TEST") != "1" {
		t.Skip("requires Sigstore TUF network access")
	}
	if err := authenticate("testdata/release"); err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	signature, err := os.ReadFile("testdata/release/SHA256SUMS.sigstore.json")
	if err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(dir, "SHA256SUMS.sigstore.json"), signature, 0600)
	os.WriteFile(filepath.Join(dir, "SHA256SUMS"), []byte("modified manifest"), 0600)
	if authenticate(dir) == nil {
		t.Fatal("accepted modified signed manifest")
	}
}
