import { defineConfig } from "vitest/config";

export default defineConfig({
	test: {
		testTimeout: 30000,
		hookTimeout: 60000,
		// Run tests sequentially to avoid race conditions with the worker
		pool: "forks",
		poolOptions: {
			forks: {
				singleFork: true,
			},
		},
	},
});
