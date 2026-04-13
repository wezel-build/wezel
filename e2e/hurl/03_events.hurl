# Create project for events test
POST {{base_url}}/api/project
Content-Type: application/json
{
  "uuid": "00000000-0000-4000-8000-000000000002",
  "name": "e2e-events-test",
  "upstream": "github.com/e2e-test/events-test"
}
HTTP 201
[Captures]
project_id: jsonpath "$.id"

# Send a pheromone build event
POST {{base_url}}/api/events
Content-Type: application/json
[
  {
    "projectId": "00000000-0000-4000-8000-000000000002",
    "upstream": "github.com/e2e-test/events-test",
    "commit": "abc1234",
    "user": "e2e-tester",
    "platform": "Linux",
    "timestamp": "2026-01-01T00:00:00Z",
    "durationMs": 5000,
    "exitCode": 0,
    "cwd": "/tmp/test",
    "pheromone": {
      "tool": "cargo",
      "command": "build",
      "profile": "dev",
      "packages": [],
      "dirtyCrates": ["test-crate"]
    }
  }
]
HTTP 200

# Observation should have been created
GET {{base_url}}/api/project/{{project_id}}/observation
HTTP 200
[Asserts]
jsonpath "$" count == 1
jsonpath "$[0].name" == "cargo build"
jsonpath "$[0].profile" == "dev"
jsonpath "$[0].runs" count == 1
jsonpath "$[0].runs[0].commit" == "abc1234"
jsonpath "$[0].runs[0].buildTimeMs" == 5000

# Project overview reflects the observation
GET {{base_url}}/api/project/{{project_id}}/overview
HTTP 200
[Asserts]
jsonpath "$.observationCount" == 1
