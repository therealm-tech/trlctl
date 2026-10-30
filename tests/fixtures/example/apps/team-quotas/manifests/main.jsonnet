local team = import '../lib/team.libsonnet';

local teams = {
  data: { cpu: '8', memory: '32Gi' },
  web: { cpu: '4', memory: '8Gi' },
};

std.flattenArrays([
  team.new(name, teams[name])
  for name in std.objectFields(teams)
])
