package main

import (
	"encoding/json"
	"os"
	"time"
)

// Progress contains only public artifact sizes and a fixed step name.
// The controller supplies a private runtime path; it never comes from a release.
func reportProgress(step string, downloaded, total int64) {
	path := os.Getenv("ELO_UPDATE_PROGRESS")
	if path == "" {
		return
	}
	value := struct {
		Step       string `json:"step"`
		Downloaded int64  `json:"downloaded"`
		Total      int64  `json:"total"`
	}{step, downloaded, total}
	data, err := json.Marshal(value)
	if err != nil {
		return
	}
	temporary := path + ".tmp"
	if os.WriteFile(temporary, data, 0600) == nil {
		if os.Rename(temporary, path) != nil {
			_ = os.Remove(temporary)
		}
	}
}

type progressWriter struct {
	downloaded int64
	total      int64
	last       time.Time
}

func (p *progressWriter) Write(data []byte) (int, error) {
	p.downloaded += int64(len(data))
	if time.Since(p.last) >= 250*time.Millisecond || p.downloaded == p.total {
		reportProgress("downloading", p.downloaded, p.total)
		p.last = time.Now()
	}
	return len(data), nil
}
