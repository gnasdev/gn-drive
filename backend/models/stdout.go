package models

import "ns-drive/backend/utils"

type RcloneStdout struct {
	Id int
	C  chan []byte
}

// Support command output
func (r RcloneStdout) Write(p []byte) (n int, err error) {
	j, e := utils.NewCommandOutputDTO(r.Id, string(p)).ToJSON()
	if e != nil {
		return 0, e
	}
	r.C <- j
	return len(p), nil
}
