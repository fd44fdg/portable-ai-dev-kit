import { existsSync } from 'node:fs';

const requestedWorkspace = process.env.PORTABLEKIT_WORKSPACE;

if (requestedWorkspace && existsSync(requestedWorkspace)) {
    process.chdir(requestedWorkspace);
}

await import('./node_modules/@iflow-ai/iflow-cli/bundle/entry.js');
