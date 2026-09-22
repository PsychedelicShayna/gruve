import copy
import unittest
from model import fresh,operation,select,patch

class ModelTests(unittest.TestCase):
    def setUp(self):self.s=fresh()
    def runop(self,**c):return operation(self.s,c)
    def dots(self):self.runop(op='create',object={'id':'d','type':'dot','color':'#0088ff'},count=10)
    def test_compact_half_selection(self):
        self.dots();ids=self.runop(op='remove',select={'type':'dot','fraction':.5});self.assertEqual(len(ids),5)
        self.runop(op='set',select={'type':'dot'},props={'color':'#ff0000'});self.assertEqual(len(self.s['objects']),5)
        self.assertTrue(all(o['color']=='#ff0000' for o in self.s['objects'].values()))
    def test_group_move_once(self):
        self.dots();self.runop(op='group',id='g',select=['d-0','d-1']);before=copy.deepcopy(self.s)
        self.runop(op='move',select=['g','d-0'],by=[20,30])
        for i in ['g','d-0','d-1']:self.assertEqual(self.s['objects'][i]['x'],before['objects'][i]['x']+20)
    def test_connection_cascade(self):
        self.dots();self.runop(op='link',id='edge',**{'from':'d-0','to':'d-1'});self.runop(op='remove',select='d-0');self.assertNotIn('edge',self.s['objects'])
    def test_missing_selection_rejected(self):
        with self.assertRaises(ValueError):self.runop(op='remove',select='absent')
    def test_unknown_properties_rejected(self):
        with self.assertRaises(ValueError):self.runop(op='create',object={'id':'d','type':'dot','html':'unsafe'})
    def test_nonfinite_rejected(self):
        with self.assertRaises(ValueError):self.runop(op='create',object={'id':'d','type':'dot','x':float('nan')})
    def test_empty_selection_safe(self):
        self.assertEqual(self.runop(op='remove',select={'type':'dot'}),[])
    def test_template_expansion(self):
        self.runop(op='define',name='triangle',items=[{'id':'a','type':'polygon','points':[[0,-10],[-10,10],[10,10]]}])
        self.runop(op='spawn',template='triangle',id='p',x=100,y=50)
        self.assertEqual(self.s['objects']['p']['members'],['p/a']);self.assertEqual(self.s['objects']['p/a']['x'],100)
    def test_property_patch_marks_only_changed_fields(self):
        self.dots();old=copy.deepcopy(self.s);self.runop(op='set',select='d-0',props={'color':'#ff0000'})
        self.assertEqual(patch(old,self.s)['fields']['d-0'],['color'])
    def test_duplicate_ownership_rejected(self):
        self.dots();self.runop(op='group',id='g',select=['d-0'])
        with self.assertRaises(ValueError):self.runop(op='group',id='h',select=['d-0'])
    def test_pinning_and_impulse(self):
        self.dots();self.runop(op='set',select='d-0',props={'pinned':True});self.runop(op='impulse',select='d-1',velocity=[100,0]);self.assertTrue(self.s['physics']['enabled'])
    def test_selector_slice(self):
        self.dots();self.assertEqual(select(self.s,{'type':'dot','slice':[2,5]}),['d-2','d-3','d-4'])
    def test_connection_cycle_rejected(self):
        with self.assertRaises(ValueError):self.runop(op='create',object={'id':'bad','type':'line','from':'bad','to':'bad'})
    def test_one_endpoint_rejected(self):
        self.dots()
        with self.assertRaises(ValueError):self.runop(op='create',object={'id':'bad','type':'line','from':'d-0'})
    def test_view_requires_a_target_and_resolves_fit_selection(self):
        self.dots()
        with self.assertRaises(ValueError):self.runop(op='view')
        with self.assertRaises(ValueError):self.runop(op='view',zoom=50)
        self.assertEqual(self.runop(op='view',fit={'type':'dot','limit':2}),['d-0','d-1'])
        self.assertEqual(self.runop(op='view',center=[10,20],zoom=2),[])
    def test_link_heads_and_routes_validated(self):
        self.dots();self.runop(op='link',id='e',arrow=True,props={'route':'curve','tail':'dot'},**{'from':'d-0','to':'d-1'})
        self.assertEqual(self.s['objects']['e']['head'],'arrow')
        with self.assertRaises(ValueError):self.runop(op='set',select='e',props={'head':'zigzag'})
        with self.assertRaises(ValueError):self.runop(op='set',select='e',props={'route':'teleport'})
    def test_set_null_deletes_a_field_and_measured_is_read_only(self):
        self.runop(op='create',object={'id':'c','type':'card','height':300});self.runop(op='set',select='c',props={'height':None})
        self.assertNotIn('height',self.s['objects']['c'])
        with self.assertRaises(ValueError):self.runop(op='set',select='c',props={'measured':{'h':1}})
        with self.assertRaises(ValueError):self.runop(op='create',object={'id':'m','type':'dot','measured':{'h':1}})

if __name__=='__main__':unittest.main()
