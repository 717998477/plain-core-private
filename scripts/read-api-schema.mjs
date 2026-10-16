import { readdirSync, readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export function readApiSchema(directory = fileURLToPath(new URL('../schema/api/', import.meta.url))) {
  const roots = new Map()
  const definitions = readdirSync(directory).filter((name) => name.endsWith('.graphqls')).sort().map((name) => {
    return readFileSync(join(directory, name), 'utf8').replace(/^(?:extend )?type (Query|Mutation) \{\n([\s\S]*?)^\}/gm, (_, name, fields) => {
      roots.set(name, [...(roots.get(name) ?? []), fields.trimEnd()])
      return ''
    }).trim()
  }).filter(Boolean)
  for (const [name, fields] of [...roots].sort(([a], [b]) => a.localeCompare(b))) {
    definitions.push(`type ${name} {\n${fields.join('\n')}\n}`)
  }
  return definitions.join('\n\n') + '\n'
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.stdout.write(readApiSchema())
}
