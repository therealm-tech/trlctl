// Per-team namespace scaffolding: the namespace itself and the quota that
// bounds it. Every object carries the team label network policies select on.
{
  // Label selecting every object owned by a team. Changing it orphans the
  // namespaces from the network policies that match on it.
  teamLabel:: 'platform.example.com/team',

  // The Namespace, ResourceQuota and LimitRange for one team, as a list.
  // `name` is the team name (the namespace is `team-<name>`); `team` must
  // carry `cpu` and `memory` (quantity strings) for the namespace quota.
  new(name, team):: [
    {
      apiVersion: 'v1',
      kind: 'Namespace',
      metadata: {
        name: 'team-%s' % name,
        labels: { [$.teamLabel]: name },
      },
    },
    {
      apiVersion: 'v1',
      kind: 'ResourceQuota',
      metadata: {
        name: 'team',
        namespace: 'team-%s' % name,
        labels: { [$.teamLabel]: name },
      },
      spec: {
        hard: {
          'requests.cpu': team.cpu,
          'requests.memory': team.memory,
          'limits.memory': team.memory,
        },
      },
    },
    {
      apiVersion: 'v1',
      kind: 'LimitRange',
      metadata: {
        name: 'team',
        namespace: 'team-%s' % name,
        labels: { [$.teamLabel]: name },
      },
      spec: {
        limits: [
          {
            type: 'Container',
            defaultRequest: { cpu: '50m', memory: '64Mi' },
            default: { memory: '128Mi' },
          },
        ],
      },
    },
  ],
}
