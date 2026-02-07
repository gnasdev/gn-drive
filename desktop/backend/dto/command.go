package dto

import "encoding/json"

type Command string

const (
	CommandStoped  Command = "command_stoped"
	CommandOutput  Command = "command_output"
	CommandStarted Command = "command_started"
	Error          Command = "error"
	SyncStatus     Command = "sync_status"
)

func (c Command) String() string {
	return string(c)
}

type CommandDTO struct {
	Command string  `json:"command"`
	Id      *int    `json:"pid,omitempty"`
	Error   *string `json:"error,omitempty"`
	Task    *string `json:"task"`
	TabId   *string `json:"tab_id,omitempty"`
}

func (c CommandDTO) ToJSON() ([]byte, error) {
	jsonData, err := json.Marshal(c)
	if err != nil {
		return []byte{}, err
	}
	return jsonData, nil
}
