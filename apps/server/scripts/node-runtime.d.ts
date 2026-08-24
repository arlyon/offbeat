declare module "node:fs" {
	export function readFileSync(path: string, encoding: "utf-8"): string;
}

declare const process: {
	argv: string[];
	env: Record<string, string | undefined>;
	exit(code?: number): never;
};
