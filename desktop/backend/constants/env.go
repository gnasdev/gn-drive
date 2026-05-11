package constants

// GN Drive note: Keeps backend constants in one place for platform and environment handling.

type Environment int

const (
	Development Environment = iota
	Production
)

func (e Environment) String() string {
	return [...]string{"development", "production"}[e]
}
