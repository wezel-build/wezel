# Create a project
POST {{base_url}}/api/project
Content-Type: application/json
{
  "name": "e2e-projects-test",
  "upstream": "github.com/e2e-test/projects-test"
}
HTTP 201
[Captures]
project_id: jsonpath "$.id"
[Asserts]
jsonpath "$.name" == "e2e-projects-test"
jsonpath "$.upstream" == "github.com/e2e-test/projects-test"

# List projects — created project must appear
GET {{base_url}}/api/project
HTTP 200
[Asserts]
jsonpath "$[*].upstream" includes "github.com/e2e-test/projects-test"

# Project overview
GET {{base_url}}/api/project/{{project_id}}/overview
HTTP 200
[Asserts]
jsonpath "$.observationCount" == 0
jsonpath "$.trackedCount" == 0
