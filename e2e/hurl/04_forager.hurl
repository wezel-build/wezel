# Create project for forager test
POST {{base_url}}/api/project
Content-Type: application/json
{
  "uuid": "00000000-0000-4000-8000-000000000003",
  "name": "e2e-forager-test",
  "upstream": "github.com/e2e-test/forager-test"
}
HTTP 201
[Captures]
project_id: jsonpath "$.id"

# Push first commit via webhook (no secret configured → no signature needed)
POST {{base_url}}/api/webhooks/github
Content-Type: application/json
X-GitHub-Event: push
{
  "ref": "refs/heads/main",
  "before": "0000000000000000000000000000000000000000",
  "after": "aabbccd1234567890000000000000000000000aa",
  "repository": { "html_url": "github.com/e2e-test/forager-test" },
  "commits": [
    {
      "id": "aabbccd1234567890000000000000000000000aa",
      "message": "First test commit",
      "timestamp": "2026-01-01T00:00:00Z",
      "author": { "username": "e2e-tester" }
    }
  ]
}
HTTP 200

# --- First run ---

# Enqueue a forager job for the first commit
POST {{base_url}}/api/forager/jobs
Content-Type: application/json
{
  "project_upstream": "github.com/e2e-test/forager-test",
  "project_uuid": "00000000-0000-4000-8000-000000000003",
  "commit_sha": "aabbccd1234567890000000000000000000000aa",
  "experiment_name": "e2e-bench"
}
HTTP 201
[Captures]
job1_id: jsonpath "$.id"
[Asserts]
jsonpath "$.status" == "pending"

# Claim the next pending job
POST {{base_url}}/api/forager/jobs/next
Content-Type: application/json
{
  "project_upstream": "github.com/e2e-test/forager-test"
}
HTTP 200
[Captures]
token1: jsonpath "$.token"
[Asserts]
jsonpath "$.commitSha" == "aabbccd1234567890000000000000000000000aa"
jsonpath "$.experimentName" == "e2e-bench"
jsonpath "$.projectId" == {{project_id}}

# Submit measurements for the first run
POST {{base_url}}/api/forager/run
Content-Type: application/json
{
  "token": "{{token1}}",
  "steps": [
    {
      "step": "baseline",
      "measurements": [
        {
          "name": "build-time",
          "kind": "duration",
          "value": 1234.5,
          "unit": "ms"
        }
      ]
    }
  ]
}
HTTP 200

# Mark job complete
PATCH {{base_url}}/api/forager/jobs/{{job1_id}}
Content-Type: application/json
{
  "status": "complete"
}
HTTP 200

# Verify measurements are stored
GET {{base_url}}/api/project/{{project_id}}/commit/aabbccd1234567890000000000000000000000aa
HTTP 200
[Asserts]
jsonpath "$.measurements" count == 1
jsonpath "$.measurements[0].name" == "build-time"
jsonpath "$.measurements[0].value" == 1234.5
jsonpath "$.measurements[0].step" == "baseline"

# --- Second run ---

# Push second commit via webhook
POST {{base_url}}/api/webhooks/github
Content-Type: application/json
X-GitHub-Event: push
{
  "ref": "refs/heads/main",
  "before": "aabbccd1234567890000000000000000000000aa",
  "after": "bbccdd112345670000000000000000000000bbcc",
  "repository": { "html_url": "github.com/e2e-test/forager-test" },
  "commits": [
    {
      "id": "bbccdd112345670000000000000000000000bbcc",
      "message": "Second test commit",
      "timestamp": "2026-01-02T00:00:00Z",
      "author": { "username": "e2e-tester" }
    }
  ]
}
HTTP 200

# Enqueue a forager job for the second commit
POST {{base_url}}/api/forager/jobs
Content-Type: application/json
{
  "project_upstream": "github.com/e2e-test/forager-test",
  "project_uuid": "00000000-0000-4000-8000-000000000003",
  "commit_sha": "bbccdd112345670000000000000000000000bbcc",
  "experiment_name": "e2e-bench"
}
HTTP 201
[Captures]
job2_id: jsonpath "$.id"

# Claim the next pending job
POST {{base_url}}/api/forager/jobs/next
Content-Type: application/json
{
  "project_upstream": "github.com/e2e-test/forager-test"
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
      "measurements": [
        {
          "name": "build-time",
          "kind": "duration",
          "value": 2000.0,
          "unit": "ms"
        }
      ]
    }
  ]
}
HTTP 200

# Mark job complete
PATCH {{base_url}}/api/forager/jobs/{{job2_id}}
Content-Type: application/json
{
  "status": "complete"
}
HTTP 200

# Second commit should have value stored (no prevValue — comparisons are frontend-driven)
GET {{base_url}}/api/project/{{project_id}}/commit/bbccdd112345670000000000000000000000bbcc
HTTP 200
[Asserts]
jsonpath "$.measurements[0].value" == 2000.0
jsonpath "$.measurements[0].prevValue" not exists
