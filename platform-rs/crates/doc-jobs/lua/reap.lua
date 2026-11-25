-- Re-queue deliveries whose worker is gone: pending for at least min_idle and no live lease.
-- A delivery held by the calling worker is skipped while its job is running: that worker is alive
-- (its lease can only have lapsed during a store outage, and its own heartbeat will re-take it).
-- KEYS: stream, stats hash; ARGV: group, job_key_prefix, lease_key_prefix, min_idle_ms, max, own_consumer
local pend = redis.call('XPENDING', KEYS[1], ARGV[1], 'IDLE', ARGV[4], '-', '+', ARGV[5])
local n = 0
for _, p in ipairs(pend) do
  local id = p[1]
  local r = redis.call('XRANGE', KEYS[1], id, id)
  if #r == 0 then
    redis.call('XACK', KEYS[1], ARGV[1], id)
  else
    local task = r[1][2][2]
    local job = ARGV[2] .. task
    local st = redis.call('HGET', job, 'st')
    local mine_and_running = (p[2] == ARGV[6]) and (st == 'running')
    if not mine_and_running and redis.call('EXISTS', ARGV[3] .. task) == 0 then
      redis.call('XACK', KEYS[1], ARGV[1], id)
      redis.call('XDEL', KEYS[1], id)
      if st == 'running' or st == 'queued' then
        redis.call('HSET', job, 'st', 'queued', 'owner', '')
        redis.call('XADD', KEYS[1], '*', 'id', task)
        redis.call('HINCRBY', KEYS[2], 'stalled_recovered', 1)
        n = n + 1
      end
    end
  end
end
return n
