package main

import (
	"fmt"
	"os"
)

// version is set at build time via -ldflags "-X main.version=...".
var version = "dev"

func greet(name string) string {
	return fmt.Sprintf("hello from __NAME__, %s", name)
}

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--version" {
		fmt.Printf("__NAME__ %s\n", version)
		return
	}
	name := "world"
	if len(os.Args) > 1 {
		name = os.Args[1]
	}
	fmt.Println(greet(name))
}
