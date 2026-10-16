import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const args = process.argv.slice(2)
if (args.some((arg) => arg !== '--write')) {
  console.error('Usage: node scripts/gen-schema.mjs [--write]')
  process.exit(1)
}
const env = { ...process.env }
delete env.UPDATE_GRAPHQL_SCHEMAS
if (args.includes('--write')) env.UPDATE_GRAPHQL_SCHEMAS = '1'
for (const [command, commandArgs] of [
  [process.execPath, ['scripts/gen-db-schema-sql.mjs', ...args]],
  ['cargo', ['test', '-p', 'plain-server', '--lib', 'schema_matches_committed_sdl']],
]) {
  const result = spawnSync(command, commandArgs, { cwd: root, env, stdio: 'inherit' })
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}
