package models

import (
	"desktop/backend/config"
	"desktop/backend/utils"
	"encoding/json"
	"os"
)

type ConfigInfo struct {
	WorkingDir           string        `json:"working_dir"`
	SelectedProfileIndex uint          `json:"selected_profile_index"`
	Profiles             Profiles      `json:"profiles"`
	EnvConfig            config.Config `json:"env_config"`
}

func (c ConfigInfo) ToJSON() ([]byte, error) {
	jsonData, err := json.Marshal(c)
	if err != nil {
		return []byte{}, err
	}
	return jsonData, nil
}

func (c *ConfigInfo) ReadFromFile(cf config.Config) error {
	profilesByteValue, err := utils.ReadFromFile(cf.ProfileFilePath)
	if err != nil {
		return err
	}

	profiles := Profiles{}
	err = json.Unmarshal(profilesByteValue, &profiles)
	if err != nil {
		return err
	}

	c.Profiles = profiles

	return nil
}

func (c *ConfigInfo) WriteToFile(cf config.Config) error {
	profilesJson, err := c.Profiles.ToJSON()
	if err != nil {
		return err
	}

	err = os.WriteFile(cf.ProfileFilePath, profilesJson, 0644)
	if err != nil {
		return err
	}

	return nil
}
