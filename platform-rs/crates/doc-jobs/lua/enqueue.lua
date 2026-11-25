-- KEYS[1] in-flight key for the dedup key, KEYS[2] job hash, KEYS[3] stream
-- ARGV: task_id, kind, payload, dedup, in_flight_ttl_ms, job_key_prefix, request_id
-- Returns {task_id, coalesced}
local existing = redis.call('GET', KEYS[1])
if existing then
  local st = redis.call('HGET', ARGV[6] .. existing, 'st')
  if st and st ~= 'done' and st ~= 'failed' then
    return {existing, 1}
  end
end
local t = redis.call('TIME')
redis.call('HSET', KEYS[2], 'st', 'queued', 'kind', ARGV[2], 'payload', ARGV[3], 'dedup', ARGV[4],
  'rid', ARGV[7], 'attempt', 0, 'progress', '', 'retrying', 0, 'owner', '', 'created_ms', t[1] * 1000 + math.floor(t[2] / 1000))
redis.call('SET', KEYS[1], ARGV[1], 'PX', ARGV[5])
redis.call('XADD', KEYS[3], '*', 'id', ARGV[1])
return {ARGV[1], 0}
