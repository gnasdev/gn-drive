package config

type Config struct {
	DebugMode       bool   `mapstructure:"DEBUG_MODE"`
	ProfileFilePath string `mapstructure:"PROFILE_FILE_PATH"`
	ResyncFilePath  string `mapstructure:"RESYNC_FILE_PATH"`
	RcloneFilePath  string `mapstructure:"RCLONE_FILE_PATH"`
}
