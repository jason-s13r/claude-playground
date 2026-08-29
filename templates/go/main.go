package main

import (
	"fmt"
	"os"
)

func greet(name string) string {
	return fmt.Sprintf("hello from __NAME__, %s", name)
}

func main() {
	name := "world"
	if len(os.Args) > 1 {
		name = os.Args[1]
	}
	fmt.Println(greet(name))
}
