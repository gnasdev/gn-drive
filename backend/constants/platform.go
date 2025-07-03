package constants

type Platform int

const (
	Windows Platform = iota
	Darwin
	Linux
)

func (p Platform) String() string {
	return [...]string{"windows", "darwin", "linux"}[p]
}
