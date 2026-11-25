-- The at-most-once email record. KEYS[1] the record. ARGV: mode, owner, ttl_s
-- mode claim: {granted | sent | doubt}; mode sent: mark sent if this owner holds the claim; mode release: delete if this owner holds it
local v = redis.call('GET', KEYS[1])
local mine = 'sending:' .. ARGV[2]
if ARGV[1] == 'claim' then
  if not v then
    redis.call('SET', KEYS[1], mine, 'EX', ARGV[3])
    return 'granted'
  end
  if v == 'sent' then return 'sent' end
  if v == mine then return 'granted' end
  return 'doubt'
elseif ARGV[1] == 'sent' then
  if v == mine or not v then
    redis.call('SET', KEYS[1], 'sent', 'EX', ARGV[3])
    return 'ok'
  end
  return 'lost'
else
  if v == mine then
    redis.call('DEL', KEYS[1])
    return 'ok'
  end
  return 'kept'
end
