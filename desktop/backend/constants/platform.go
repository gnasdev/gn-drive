package constants

// GN Drive note: Keeps backend constants in one place for platform and environment handling.

type Platform int

const (
	Windows Platform = iota
	Darwin
	Linux
)

func (p Platform) String() string {
	return [...]string{"windows", "darwin", "linux"}[p]
}
