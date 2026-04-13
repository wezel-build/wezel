# Create a project
POST {{base_url}}/api/project
Content-Type: application/json
{
  "uuid": "00000000-0000-4000-8000-000000000001",
  "name": "e2e-projects-test",
  "upstream": "github.com/e2e-test/projects-test"
}
HTTP 201
[Captures]
project_id: jsonpath "$.id"
[Asserts]
jsonpath "$.uuid" == "00000000-0000-4000-8000-000000000001"
jsonpath "$.name" == "e2e-projects-test"
jsonpath "$.upstream" == "github.com/e2e-test/projects-test"

# Idempotent re-registration returns the same project
POST {{base_url}}/api/project
Content-Type: application/json
{
  "uuid": "00000000-0000-4000-8000-000000000001",
  "name": "e2e-projects-test",
  "upstream": "github.com/e2e-test/projects-test"
}
HTTP 200
[Asserts]
jsonpath "$.id" == {{project_id}}
jsonpath "$.uuid" == "00000000-0000-4000-8000-000000000001"

# List projects — created project must appear
GET {{base_url}}/api/project
HTTP 200
[Asserts]
jsonpath "$[*].upstream" contains "github.com/e2e-test/projects-test"

# Project overview
GET {{base_url}}/api/project/{{project_id}}/overview
HTTP 200
[Asserts]
jsonpath "$.observationCount" == 0
jsonpath "$.trackedCount" == 0
