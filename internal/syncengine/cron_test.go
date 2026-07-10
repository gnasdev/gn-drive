package syncengine

import "testing"

func TestNormalizeCron_FiveField(t *testing.T) {
	got, err := NormalizeCron("0 * * * *")
	if err != nil {
		t.Fatal(err)
	}
	if got != "0 0 * * * *" {
		t.Fatalf("got %q, want %q", got, "0 0 * * * *")
	}
}

func TestNormalizeCron_SixField(t *testing.T) {
	got, err := NormalizeCron("15 0 * * * *")
	if err != nil {
		t.Fatal(err)
	}
	if got != "15 0 * * * *" {
		t.Fatalf("got %q", got)
	}
}

func TestNormalizeCron_Descriptor(t *testing.T) {
	if _, err := NormalizeCron("@hourly"); err != nil {
		t.Fatal(err)
	}
}

func TestNormalizeCron_Invalid(t *testing.T) {
	for _, expr := range []string{"", "not-a-cron", "a b c", "* *"} {
		if _, err := NormalizeCron(expr); err == nil {
			t.Errorf("%q: expected error", expr)
		}
	}
}
