package dto

type AppError struct {
	Message string `json:"message"`
}

func NewAppError(e error) *AppError {
	if e == nil {
		return nil
	}

	return &AppError{
		Message: e.Error(),
	}
}
