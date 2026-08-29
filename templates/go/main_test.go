package main

import "testing"

func TestGreet(t *testing.T) {
	got := greet("world")
	want := "hello from __NAME__, world"
	if got != want {
		t.Errorf("greet() = %q, want %q", got, want)
	}
}
