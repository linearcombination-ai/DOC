-- complete (mode "done") or permanently fail (mode "failed")
-- KEYS: job hash, lease key, in-flight key, stream
-- ARGV: owner, mode, result_or_failure_json, ttl_s, stream_id, task_id, group
if redis.call('HGET', KEYS[1], 'owner') ~= ARGV[1] or redis.call('HGET', KEYS[1], 'st') ~= 'running' then return 0 end
if ARGV[2] == 'done' then
  redis.call('HSET', KEYS[1], 'st', 'done', 'result', ARGV[3], 'owner', '', 'retrying', 0)
else
  redis.call('HSET', KEYS[1], 'st', 'failed', 'failure', ARGV[3], 'owner', '', 'retrying', 0)
end
redis.call('DEL', KEYS[2])
if redis.call('GET', KEYS[3]) == ARGV[6] then redis.call('DEL', KEYS[3]) end
redis.call('XACK', KEYS[4], ARGV[7], ARGV[5])
redis.call('XDEL', KEYS[4], ARGV[5])
redis.call('EXPIRE', KEYS[1], ARGV[4])
return 1
