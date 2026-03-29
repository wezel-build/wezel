# Create project for forager test
POST {{base_url}}/api/project
Content-Type: application/json
{
  "name": "e2e-forager-test",
  "upstream": "github.com/e2e-test/forager-test"
}
HTTP 201
[Captures]
project_id: jsonpath "$.id"

# --- First run ---

# Claim a forager job for the first commit
POST {{base_url}}/api/forager/claim
Content-Type: application/json
{
  "project_upstream": "github.com/e2e-test/forager-test",
  "commit_sha": "aabbccd1234567",
  "benchmark_name": "e2e-bench",
  "commit_author": "e2e-tester",
  "commit_message": "First test commit",
  "commit_timestamp": "2026-01-01T00:00:00Z"
}
HTTP 200
[Captures]
token1: jsonpath "$.token"
[Asserts]
jsonpath "$.commitSha" == "aabbccd1234567"
jsonpath "$.benchmarkName" == "e2e-bench"
jsonpath "$.projectId" == {{project_id}}

# Submit measurements for the first run
POST {{base_url}}/api/forager/run
Content-Type: application/json
{
  "token": "{{token1}}",
  "steps": [
    {
      "step": "baseline",
      "measurement": {
        "name": "build-time",
        "kind": "duration",
        "value": 1234.5,
        "unit": "ms"
      }
    }
  ]
}
HTTP 200

# Verify measurements are stored
GET {{base_url}}/api/project/{{project_id}}/commit/aabbccd1234567
HTTP 200
[Asserts]
jsonpath "$.measurements" count == 1
jsonpath "$.measurements[0].name" == "build-time"
jsonpath "$.measurements[0].value" == 1234.5
jsonpath "$.measurements[0].step" == "baseline"

# --- Second run ---

# Claim a forager job for the second commit
POST {{base_url}}/api/forager/claim
Content-Type: application/json
{
  "project_upstream": "github.com/e2e-test/forager-test",
  "commit_sha": "bbccdd11234567",
  "benchmark_name": "e2e-bench",
  "commit_author": "e2e-tester",
  "commit_message": "Second test commit",
  "commit_timestamp": "2026-01-02T00:00:00Z"
}
HTTP 200
[Captures]
token2: jsonpath "$.token"

# Submit same measurement name for second run
POST {{base_url}}/api/forager/run
Content-Type: application/json
{
  "token": "{{token2}}",
  "steps": [
    {
      "step": "baseline",
      "measurement": {
        "name": "build-time",
        "kind": "duration",
        "value": 2000.0,
        "unit": "ms"
      }
    }
  ]
}
HTTP 200

# Second commit should have value stored (no prevValue — comparisons are frontend-driven)
GET {{base_url}}/api/project/{{project_id}}/commit/bbccdd11234567
HTTP 200
[Asserts]
jsonpath "$.measurements[0].value" == 2000.0
jsonpath "$.measurements[0].prevValue" not exists
