package models

import "encoding/json"

type Remote struct {
	Name  string         `json:"name"`
	Type  string         `json:"type"`
	Token map[string]any `json:"token"`
}

type Remotes []Remote

func (remotes Remotes) ToJSON() ([]byte, error) {
	jsonData, err := json.MarshalIndent(remotes, "", "    ")
	if err != nil {
		return []byte{}, err
	}
	return jsonData, nil
}
