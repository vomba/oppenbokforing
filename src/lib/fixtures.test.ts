import Ajv2020 from "ajv/dist/2020.js"
import addFormats from "ajv-formats"
import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"

const fixturesDir = join(process.cwd(), "fixtures/golden-scenarios")
const schema = JSON.parse(
  readFileSync(join(fixturesDir, "schema.json"), "utf8"),
) as object

const ajv = new Ajv2020({ allErrors: true, strict: false })
addFormats(ajv)
const validate = ajv.compile(schema)

describe("golden scenario fixtures", () => {
  const ids = readdirSync(fixturesDir)
    .filter((name) => name.endsWith(".json") && name !== "schema.json")
    .map((name) => name.replace(/\.json$/, ""))

  it("lists at least the RFC baseline scenarios", () => {
    expect(ids).toEqual(
      expect.arrayContaining([
        "fa-skatt-salary-and-business",
        "vat-exempt-below-threshold",
        "year-end-k1-ne",
      ]),
    )
  })

  it.each(ids)("%s validates against JSON Schema and filename contract", (id) => {
    const raw = readFileSync(join(fixturesDir, `${id}.json`), "utf8")
    const fixture = JSON.parse(raw) as {
      id: string
      title: string
      milestone?: number
      rfcSection?: string
      sources: string[]
    }

    const valid = validate(fixture)
    if (!valid) {
      const details = validate.errors?.map((e) => `${e.instancePath} ${e.message}`).join("; ")
      throw new Error(`Schema validation failed for ${id}: ${details}`)
    }

    expect(fixture.id).toBe(id)
    expect(fixture.title.length).toBeGreaterThan(0)
    expect(fixture.sources.length).toBeGreaterThan(0)
    for (const url of fixture.sources) {
      expect(url).toMatch(/^https?:\/\//)
    }
    expect(fixture.milestone).toBeDefined()
    expect(fixture.rfcSection?.length).toBeGreaterThan(0)
  })
})
