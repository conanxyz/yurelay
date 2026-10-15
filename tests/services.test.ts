import { describe, expect, it } from 'vitest';
import { serviceEntries, serviceName } from '../src/services';
import { defaults } from '../src/types';

describe('service catalogue', () => {
  const custom = { ...defaults.profiles[0], id: 'custom', name: '自定义模型' };
  const zhipu = { ...custom, id: 'zhipu', provider: 'zhipu' as const, name: '智谱' };
  it('pins the default across configured and available providers and puts custom last', () => {
    const entries = serviceEntries([custom, zhipu], 'zhipu', '');
    expect(entries[0].profile?.id).toBe('zhipu');
    expect(entries.at(-1)?.profile?.id).toBe('custom');
    expect(entries.filter(e => (e.profile?.provider ?? e.preset?.provider) === 'zhipu')).toHaveLength(1);
    expect(serviceEntries([custom, zhipu], 'custom', '')[0].profile?.id).toBe('custom');
    expect(serviceEntries([zhipu], 'zhipu', '').at(-1)?.preset?.provider).toBe('custom');
    expect(serviceEntries([custom,zhipu], 'zhipu', 'zidingyi')).toHaveLength(1);
  });
  it('localizes built-in names and preserves personal names', () => {
    expect(serviceName(custom, 'en')).toBe('Custom service');
    expect(serviceName(custom, 'zh')).toBe('自定义服务');
    expect(serviceName({...custom,name:'我的代理'}, 'en')).toBe('我的代理');
  });
});
