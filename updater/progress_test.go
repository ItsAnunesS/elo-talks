package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestDownloadProgressUsesActualBytes(t *testing.T) {
	path := filepath.Join(t.TempDir(), "progress.json")
	t.Setenv("ELO_UPDATE_PROGRESS", path)
	writer := &progressWriter{total: 10}
	if n, err := writer.Write([]byte("1234")); n != 4 || err != nil {
		t.Fatal(n, err)
	}
	if n, err := writer.Write([]byte("567890")); n != 6 || err != nil {
		t.Fatal(n, err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var result struct {
		Step              string
		Downloaded, Total int64
	}
	if err := json.Unmarshal(data, &result); err != nil {
		t.Fatal(err)
	}
	if result.Step != "downloading" || result.Downloaded != 10 || result.Total != 10 {
		t.Fatal(result)
	}
	info, _ := os.Stat(path)
	if info.Mode().Perm() != 0600 {
		t.Fatal("Progress must be private")
	}
}
