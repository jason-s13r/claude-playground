export function greet(name: string): string {
  return `hello from __NAME__, ${name}`;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  console.log(greet(process.argv[2] ?? "world"));
}
