package dto

// GN Drive note: Defines transport objects passed between backend commands and the frontend.

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
