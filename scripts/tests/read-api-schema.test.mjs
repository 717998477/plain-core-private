import { strict as assert } from 'node:assert'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { readApiSchema } from '../read-api-schema.mjs'

test('combines feature roots while preserving arguments, descriptions and shared types', () => {
  const directory = mkdtempSync(join(tmpdir(), 'plain-schema-'))
  try {
    writeFileSync(join(directory, 'common.graphqls'), 'scalar Long\n')
    writeFileSync(join(directory, 'audio.graphqls'), 'type Query {\n\t"""Audio { details }"""\n\taudio(id: ID!): Long!\n}\n\ntype Mutation {\n\tplay(id: ID!): Boolean!\n}\n')
    writeFileSync(join(directory, 'files.graphqls'), 'type File {\n\tpath: String!\n}\n\nextend type Query {\n\tfiles(query: String! = ""): [File!]!\n}\n\nextend type Mutation {\n\ttrash(query: String!): Boolean!\n}\n')
    writeFileSync(join(directory, 'ignored.txt'), 'ignored')
    const sdl = readApiSchema(directory)
    assert.equal((sdl.match(/type Query \{/g) ?? []).length, 1)
    assert.equal((sdl.match(/type Mutation \{/g) ?? []).length, 1)
    assert.ok(!sdl.includes('extend type'))
    assert.ok(sdl.includes('"""Audio { details }"""\n\taudio(id: ID!): Long!'))
    assert.ok(sdl.includes('files(query: String! = ""): [File!]!'))
    assert.ok(sdl.includes('trash(query: String!): Boolean!'))
    assert.ok(sdl.includes('type File {\n\tpath: String!\n}'))
    assert.ok(sdl.includes('scalar Long'))
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})
