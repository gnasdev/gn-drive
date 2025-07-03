package utils

import "desktop/backend/dto"

func NewCommandStoppedDTO(pid int) dto.CommandDTO {
	return dto.CommandDTO{Command: dto.CommandStoped.String(), Id: &pid}
}

func NewCommandStoppedDTOWithTab(pid int, tabId string) dto.CommandDTO {
	return dto.CommandDTO{Command: dto.CommandStoped.String(), Id: &pid, TabId: &tabId}
}

func NewCommandOutputDTO(pid int, output string) dto.CommandDTO {
	return dto.CommandDTO{Command: dto.CommandOutput.String(), Id: &pid, Error: &output}
}

func NewCommandOutputDTOWithTab(pid int, output string, tabId string) dto.CommandDTO {
	return dto.CommandDTO{Command: dto.CommandOutput.String(), Id: &pid, Error: &output, TabId: &tabId}
}

func NewCommandErrorDTO(pid int, err error) dto.CommandDTO {
	errorString := err.Error()
	return dto.CommandDTO{Command: dto.Error.String(), Id: &pid, Error: &errorString}
}

func NewCommandErrorDTOWithTab(pid int, err error, tabId string) dto.CommandDTO {
	errorString := err.Error()
	return dto.CommandDTO{Command: dto.Error.String(), Id: &pid, Error: &errorString, TabId: &tabId}
}

func NewCommandStartedDTO(pid int, task string) dto.CommandDTO {
	return dto.CommandDTO{Command: dto.CommandStarted.String(), Id: &pid, Task: &task}
}

func NewCommandStartedDTOWithTab(pid int, task string, tabId string) dto.CommandDTO {
	return dto.CommandDTO{Command: dto.CommandStarted.String(), Id: &pid, Task: &task, TabId: &tabId}
}

func NewSyncStatusDTO(pid int, action string) dto.SyncStatusDTO {
	return *dto.NewSyncStatusDTO(pid, action)
}

func NewSyncStatusDTOWithTab(pid int, action string, tabId string) dto.SyncStatusDTO {
	return *dto.NewSyncStatusDTOWithTab(pid, action, tabId)
}
