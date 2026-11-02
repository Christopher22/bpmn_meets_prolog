/*  bpmn.pl -- a small, declarative BPMN 2.0 DSL for Scryer Prolog
    ==================================================================

    A model is ONE Prolog term:

        bpmn(DefinitionsId, [ Pool | MessageFlow | Process ... ])

    Pools / processes
        pool(Id, Name, Body)        pool with an internal process
        pool(Id, Name)              black-box pool (no process)
        process(Id, Name, Body)     bare process (no collaboration)

    Body items (inside a pool, process or subprocess)
        Events     start(Id[,Name[,Trigger]])      end(Id[,Name[,Trigger]])
                   catch(Id, Name, Trigger)        throw(Id, Name, Trigger)
                   boundary(Id, Name, Trigger, AttachedTo)
                       Trigger = non_interrupting(T) for a non-interrupting one
        Triggers   none | message | signal | error | escalation | compensation
                   terminate | timer(Duration) | timer(date(D)) | timer(cycle(C))
                   conditional(Expr) | link(Name)
        Activities task(Id, Name[, Kind])   Kind = user | service | script | send
                                            | receive | manual | business_rule
                   subprocess(Id, Name, Body)
                   call(Id, Name, CalledElement)
        Gateways   xor(Id[,Name])  and(Id[,Name])  or(Id[,Name])
                   event_based(Id[,Name])  complex(Id[,Name])
        Lanes      lane(Id, Name, [MemberId, ...])
        Flows      a => b => c                  chain of sequence flows
                   g => [x, y => z]             fan out (list = branches)
                   [x, y] => join               fan in
                   g => "yes":x                 labelled flow
                   g => when("ok", "${ok}"):x   labelled + conditionExpression
                   g => default:x               default flow
    Top level
        a ~> b   |  a ~> "label":b             message flow (node or pool ids)

    Main API
        bpmn_db(+Model, -Facts)       normalise sugar into a flat fact list
        bpmn_fact(+Model, ?Fact)      query the normalised model
        bpmn_check(+Model, -Problems) structural / BPMN well-formedness lint
        bpmn_xml(+Model, -XmlTerm)    model -> element(Name, Attrs, Children)
        xml_document(+XmlTerm)//      XmlTerm -> characters
        bpmn_chars(+Model, -Chars)    model -> BPMN 2.0 XML text
        bpmn_write(+Model, +File)     model -> .bpmn file (with diagram layout)
*/

:- module(bpmn, [ bpmn_db/2,
                  bpmn_fact/2,
                  bpmn_check/2,
                  bpmn_xml/2,
                  bpmn_chars/2,
                  bpmn_write/2,
                  xml_document//1,
                  op(700, xfy, =>),
                  op(700, xfx, ~>)
                ]).

:- use_module(library(lists)).
:- use_module(library(dcgs)).
:- use_module(library(pairs)).
:- use_module(library(assoc)).
:- use_module(library(pio)).
:- use_module(library(error)).
:- use_module(library(charsio)).
:- use_module(library(between)).

:- op(700, xfy, =>).
:- op(700, xfx, ~>).

/* =====================================================================
   1. Normalisation: surface syntax  ->  flat facts
   ---------------------------------------------------------------------
      pool(Id, Name, ProcessId|none)      process(Id, Name)
      node(Id, Type, Name, Container)     lane(Id, Name, Process, Members)
      flow(Id, Source, Target, Label, Container)
      mflow(Id, Source, Target, Label)
   ===================================================================== */

bpmn_db(bpmn(_, Items), Db) :-
    phrase(top_items(Items), Db).

bpmn_fact(Model, Fact) :-
    bpmn_db(Model, Db),
    member(Fact, Db).

top_items([]) --> [].
top_items([I|Is]) --> top_item(I), top_items(Is).

top_item(pool(P, Name, Body)) --> !,
    { cat([P, '_process'], Proc) },
    [pool(P, Name, Proc), process(Proc, Name)],
    body(Body, Proc).
top_item(pool(P, Name)) --> !, [pool(P, Name, none)].
top_item(process(Proc, Name, Body)) --> !,
    [process(Proc, Name)],
    body(Body, Proc).
top_item(S ~> T0) --> !,
    { label_target(T0, L, T), cat([mf_, S, '_', T], Id) },
    [mflow(Id, S, T, L)].
top_item(X) --> { domain_error(bpmn_top_item, X, bpmn_db/2) }.

label_target(L:T, L, T) :- !.
label_target(T, none, T).

body([], _) --> [].
body([X|Xs], C) --> item(X, C), body(Xs, C).

item(start(Id), C)          --> !, item(start(Id, none, none), C).
item(start(Id, N), C)       --> !, item(start(Id, N, none), C).
item(start(Id, N, T), C)    --> !, [node(Id, start(T), N, C)].
item(end(Id), C)            --> !, item(end(Id, none, none), C).
item(end(Id, N), C)         --> !, item(end(Id, N, none), C).
item(end(Id, N, T), C)      --> !, [node(Id, end(T), N, C)].
item(catch(Id, N, T), C)    --> !, [node(Id, catch(T), N, C)].
item(throw(Id, N, T), C)    --> !, [node(Id, throw(T), N, C)].
item(boundary(Id, N, T0, On), C) --> !,
    { interrupting(T0, T, I) },
    [node(Id, boundary(T, On, I), N, C)].
item(task(Id, N), C)        --> !, item(task(Id, N, plain), C).
item(task(Id, N, K), C)     --> !,
    { must_be_member(K, [plain, user, service, script, send, receive,
                         manual, business_rule], task_kind) },
    [node(Id, task(K), N, C)].
item(subprocess(Id, N, B), C) --> !, [node(Id, subprocess, N, C)], body(B, Id).
item(call(Id, N, Ref), C)   --> !, [node(Id, call(Ref), N, C)].
item(lane(Id, N, Ms), C)    --> !, [lane(Id, N, C, Ms)].
item(A => B, C)             --> !, chain(A => B, C, _, _).
item(G, C) -->
    { G =.. [K, Id | Rest], gateway_tag(K, _),
      ( Rest = [] -> N = none ; Rest = [N] ) }, !,
    [node(Id, gateway(K), N, C)].
item(X, _) --> { domain_error(bpmn_element, X, bpmn_db/2) }.

interrupting(non_interrupting(T), T, false) :- !.
interrupting(T, T, true).

must_be_member(X, Xs, What) :-
    (   memberchk(X, Xs) -> true
    ;   domain_error(What, X, bpmn_db/2)
    ).

%  chain(+FlowExpr, +Container, -Entries, -Exits)//
%  Entries: Label-Id pairs (where incoming flows attach), Exits: Ids.
chain(A => B, C, Ins, Outs) --> !,
    chain(A, C, Ins, Mid),
    chain(B, C, InsB, Outs),
    connect(Mid, InsB, C).
chain(L:T, C, Ins, Outs) --> !,
    chain(T, C, Ins0, Outs),
    { maplist(relabel(L), Ins0, Ins) }.
chain([], _, [], []) --> !.
chain([T|Ts], C, Ins, Outs) --> !,
    chain(T, C, I1, O1),
    chain(Ts, C, I2, O2),
    { append(I1, I2, Ins), append(O1, O2, Outs) }.
chain(Id, _, [none-Id], [Id]) --> { atom(Id) }, !.
chain(X, _, _, _) --> { domain_error(flow_expression, X, bpmn_db/2) }.

relabel(L, _-Id, L-Id).

connect([], _, _) --> [].
connect([O|Os], Ins, C) --> connect_(Ins, O, C), connect(Os, Ins, C).

connect_([], _, _) --> [].
connect_([L-T|Ins], S, C) -->
    { cat([flow_, S, '_', T], F) },
    [flow(F, S, T, L, C)],
    connect_(Ins, S, C).

/* =====================================================================
   2. Checks (pure queries over the fact list)
   ===================================================================== */

bpmn_check(Model, Problems) :-
    bpmn_db(Model, Db),
    findall(P, problem(Db, P), Ps),
    sort(Ps, Problems).

fact_id(pool(Id, _, _), Id).
fact_id(process(Id, _), Id).
fact_id(node(Id, _, _, _), Id).
fact_id(lane(Id, _, _, _), Id).
fact_id(flow(Id, _, _, _, _), Id).
fact_id(mflow(Id, _, _, _), Id).

problem(Db, duplicate_id(Id)) :-
    findall(I-x, (member(F, Db), fact_id(F, I)), Ids),
    keysort(Ids, Sorted),                 % keysort keeps duplicates
    append(_, [Id-_, Id-_ | _], Sorted).
problem(Db, invalid_id(Id)) :-
    member(F, Db), fact_id(F, Id),
    \+ ncname(Id).
problem(Db, unknown_flow_end(F, X)) :-
    member(flow(F, S, T, _, _), Db),
    member(X, [S, T]),
    \+ member(node(X, _, _, _), Db).
problem(Db, flow_crosses_container(F)) :-
    member(flow(F, S, T, _, C), Db),
    member(X, [S, T]),
    member(node(X, _, _, C1), Db), C1 \== C.
problem(Db, start_has_incoming(Id)) :-
    member(node(Id, start(_), _, _), Db), member(flow(_, _, Id, _, _), Db).
problem(Db, end_has_outgoing(Id)) :-
    member(node(Id, end(_), _, _), Db), member(flow(_, Id, _, _, _), Db).
problem(Db, boundary_has_incoming(Id)) :-
    member(node(Id, boundary(_, _, _), _, _), Db), member(flow(_, _, Id, _, _), Db).
problem(Db, no_incoming(Id)) :-
    member(node(Id, T, _, _), Db),
    \+ memberchk(T, [start(_), boundary(_, _, _)]),
    \+ member(flow(_, _, Id, _, _), Db).
problem(Db, no_outgoing(Id)) :-
    member(node(Id, T, _, _), Db),
    T \= end(_),
    \+ member(flow(_, Id, _, _, _), Db).
problem(Db, bad_attachment(B, On)) :-
    member(node(B, boundary(_, On, _), _, C), Db),
    \+ ( member(node(On, T, _, C), Db), activity(T) ).
problem(Db, bad_trigger(Id, Trigger)) :-
    member(node(Id, Type, _, _), Db),
    event_trigger(Type, Kind, Trigger),
    trigger_name(Trigger, TN),
    allowed_triggers(Kind, Allowed),
    \+ memberchk(TN, Allowed).
problem(Db, event_gateway_target(G, T)) :-
    member(node(G, gateway(event_based), _, _), Db),
    member(flow(_, G, T, _, _), Db),
    \+ member(node(T, catch(_), _, _), Db),
    \+ member(node(T, task(receive), _, _), Db).
problem(Db, default_not_allowed(F)) :-
    member(flow(F, S, _, default, _), Db),
    member(node(S, T, _, _), Db),
    \+ default_allowed(T).
problem(Db, several_defaults(S)) :-
    member(node(S, _, _, _), Db),
    findall(F, member(flow(F, S, _, default, _), Db), [_, _ | _]).
problem(Db, unknown_message_end(M, X)) :-
    member(mflow(M, S, T, _), Db),
    member(X, [S, T]),
    \+ member(node(X, _, _, _), Db),
    \+ member(pool(X, _, _), Db).
problem(Db, message_flow_inside_pool(M)) :-
    member(mflow(M, S, T, _), Db),
    pool_of(Db, S, P), pool_of(Db, T, P).
problem(Db, unknown_lane_member(L, X)) :-
    member(lane(L, _, C, Ms), Db),
    member(X, Ms),
    \+ member(node(X, _, _, C), Db).
problem(Db, not_in_any_lane(Id)) :-
    member(lane(_, _, C, _), Db),
    member(node(Id, T, _, C), Db),
    T \= boundary(_, _, _),
    \+ ( member(lane(_, _, C, Ms), Db), memberchk(Id, Ms) ).

activity(task(_)).
activity(subprocess).
activity(call(_)).

default_allowed(T) :- activity(T).
default_allowed(gateway(K)) :- memberchk(K, [xor, or, complex]).

event_trigger(start(T), start, T).
event_trigger(end(T), end, T).
event_trigger(catch(T), catch, T).
event_trigger(throw(T), throw, T).
event_trigger(boundary(T, _, _), boundary, T).

trigger_name(T, N) :- ( compound(T) -> functor(T, N, _) ; N = T ).

allowed_triggers(start,    [none, message, timer, signal, conditional]).
allowed_triggers(end,      [none, message, signal, error, escalation,
                            compensation, terminate]).
allowed_triggers(catch,    [message, timer, signal, conditional, link]).
allowed_triggers(throw,    [none, message, signal, escalation, compensation, link]).
allowed_triggers(boundary, [message, timer, signal, error, escalation,
                            conditional, compensation]).

%  pool_of(+Db, +NodeOrPool, -Pool)
pool_of(Db, X, X) :- member(pool(X, _, _), Db), !.
pool_of(Db, X, P) :-
    member(node(X, _, _, C), Db), !,
    container_pool(Db, C, P).

container_pool(Db, C, P) :- member(pool(P, _, C), Db), !.
container_pool(Db, C, P) :-
    member(node(C, subprocess, _, C1), Db),
    container_pool(Db, C1, P).

ncname(A) :-
    atom(A), atom_chars(A, [C|Cs]),
    ( char_type(C, alpha) ; C == '_' ), !,
    \+ ( member(D, Cs), \+ ( char_type(D, alnum) ; memberchk(D, ['_', '-', '.']) ) ).

/* =====================================================================
   3. Model  ->  XML term   element(Name, [Attr=Value, ...], Children)
   ===================================================================== */

bpmn_xml(Model, element('bpmn:definitions', Attrs, Kids)) :-
    Model = bpmn(Def, _),
    bpmn_db(Model, Db),
    Attrs = [ 'xmlns:bpmn'   = 'http://www.omg.org/spec/BPMN/20100524/MODEL',
              'xmlns:bpmndi' = 'http://www.omg.org/spec/BPMN/20100524/DI',
              'xmlns:dc'     = 'http://www.omg.org/spec/DD/20100524/DC',
              'xmlns:di'     = 'http://www.omg.org/spec/DD/20100524/DI',
              'xmlns:xsi'    = 'http://www.w3.org/2001/XMLSchema-instance',
              id = Def,
              targetNamespace = 'http://bpmn.io/schema/bpmn',
              exporter = 'bpmn.pl (Scryer Prolog)',
              exporterVersion = '0.1' ],
    cat([Def, '_collaboration'], Coll),
    (   member(pool(_, _, _), Db)
    ->  collaboration_xml(Db, Coll, CX), Roots = [CX], PlaneOf = Coll
    ;   Roots = [], once(member(process(PlaneOf, _), Db))
    ),
    findall(PX, ( member(process(P, N), Db), process_xml(Db, P, N, PX) ), Procs),
    diagrams_xml(Db, Def, PlaneOf, Dias),
    append([Roots, Procs, Dias], Kids).

collaboration_xml(Db, Id, element('bpmn:collaboration', [id=Id], Kids)) :-
    findall(element('bpmn:participant', [id=P, name=N, processRef=Proc], []),
            member(pool(P, N, Proc), Db), Ps),
    findall(element('bpmn:messageFlow', [id=M, name=L, sourceRef=S, targetRef=T], []),
            member(mflow(M, S, T, L), Db), Ms),
    append(Ps, Ms, Kids).

process_xml(Db, P, N, element('bpmn:process', [id=P, name=N, isExecutable=false], Kids)) :-
    findall(element('bpmn:lane', [id=L, name=LN], Refs),
            ( member(lane(L, LN, P, Ms), Db),
              findall(element('bpmn:flowNodeRef', [], [M]), member(M, Ms), Refs) ),
            Lanes),
    (   Lanes = [] -> LaneSet = []
    ;   cat([P, '_lanes'], LS),
        LaneSet = [element('bpmn:laneSet', [id=LS], Lanes)]
    ),
    flow_elements(Db, P, FEs),
    append(LaneSet, FEs, Kids).

flow_elements(Db, C, Elements) :-
    findall(X, ( member(node(Id, T, N, C), Db), node_xml(Db, node(Id, T, N, C), X) ), Ns),
    findall(X, ( member(flow(F, S, T, L, C), Db), flow_xml(flow(F, S, T, L, C), X) ), Fs),
    append(Ns, Fs, Elements).

node_xml(Db, node(Id, Type, Name, _), element(Tag, [id=Id, name=Name | TA], Kids)) :-
    type_tag(Type, Tag),
    type_attrs(Type, Db, Id, TA),
    findall(element('bpmn:incoming', [], [F]), member(flow(F, _, Id, _, _), Db), In),
    findall(element('bpmn:outgoing', [], [F]), member(flow(F, Id, _, _, _), Db), Out),
    type_children(Type, Db, Id, TK),
    append([In, Out, TK], Kids).

type_tag(start(_),          'bpmn:startEvent').
type_tag(end(_),            'bpmn:endEvent').
type_tag(catch(_),          'bpmn:intermediateCatchEvent').
type_tag(throw(_),          'bpmn:intermediateThrowEvent').
type_tag(boundary(_, _, _), 'bpmn:boundaryEvent').
type_tag(subprocess,        'bpmn:subProcess').
type_tag(call(_),           'bpmn:callActivity').
type_tag(task(K), Tag)    :- task_tag(K, Tag).
type_tag(gateway(K), Tag) :- gateway_tag(K, Tag).

task_tag(plain,         'bpmn:task').
task_tag(user,          'bpmn:userTask').
task_tag(service,       'bpmn:serviceTask').
task_tag(script,        'bpmn:scriptTask').
task_tag(send,          'bpmn:sendTask').
task_tag(receive,       'bpmn:receiveTask').
task_tag(manual,        'bpmn:manualTask').
task_tag(business_rule, 'bpmn:businessRuleTask').

gateway_tag(xor,         'bpmn:exclusiveGateway').
gateway_tag(and,         'bpmn:parallelGateway').
gateway_tag(or,          'bpmn:inclusiveGateway').
gateway_tag(event_based, 'bpmn:eventBasedGateway').
gateway_tag(complex,     'bpmn:complexGateway').

type_attrs(boundary(_, On, I), _, _, [attachedToRef=On, cancelActivity=CA]) :- !,
    ( I == false -> CA = false ; CA = none ).
type_attrs(call(Ref), Db, Id, [calledElement=Ref | D]) :- !, default_attr(Db, Id, D).
type_attrs(T, Db, Id, D) :- default_allowed(T), !, default_attr(Db, Id, D).
type_attrs(_, _, _, []).

default_attr(Db, Id, [default=F]) :- member(flow(F, Id, _, default, _), Db), !.
default_attr(_, _, []).

type_children(subprocess, Db, Id, Kids) :- !, flow_elements(Db, Id, Kids).
type_children(Type, _, Id, Defs) :-
    event_trigger(Type, _, T), !,
    event_definition(T, Id, Defs).
type_children(_, _, _, []).

event_definition(none, _, []) :- !.
event_definition(T, Id, [element(Tag, [id=DId | As], Ks)]) :-
    cat([Id, '_ed'], DId),
    edef(T, Tag, As, Ks).

edef(message,      'bpmn:messageEventDefinition',     [], []).
edef(signal,       'bpmn:signalEventDefinition',      [], []).
edef(error,        'bpmn:errorEventDefinition',       [], []).
edef(escalation,   'bpmn:escalationEventDefinition',  [], []).
edef(compensation, 'bpmn:compensateEventDefinition',  [], []).
edef(terminate,    'bpmn:terminateEventDefinition',   [], []).
edef(link(N),      'bpmn:linkEventDefinition',        [name=N], []).
edef(timer(S),     'bpmn:timerEventDefinition',       [], [element(Tag, [Xt], [E])]) :-
    formal(Xt), timer_spec(S, Tag, E).
edef(conditional(E), 'bpmn:conditionalEventDefinition', [],
     [element('bpmn:condition', [Xt], [E])]) :- formal(Xt).

timer_spec(date(E),  'bpmn:timeDate',     E) :- !.
timer_spec(cycle(E), 'bpmn:timeCycle',    E) :- !.
timer_spec(E,        'bpmn:timeDuration', E).

formal('xsi:type' = 'bpmn:tFormalExpression').

flow_xml(flow(F, S, T, L, _),
         element('bpmn:sequenceFlow', [id=F, name=N, sourceRef=S, targetRef=T], Kids)) :-
    (   L == none    -> N = none, Kids = []
    ;   L == default -> N = none, Kids = []
    ;   L = when(N, E) -> formal(Xt),
        Kids = [element('bpmn:conditionExpression', [Xt], [E])]
    ;   N = L, Kids = []
    ).

/* =====================================================================
   4. Diagram interchange: a layered (Sugiyama-style) auto-layout
      1. cycles     depth-first search; back edges are laid out reversed
      2. columns    longest-path rank, so every forward flow points right
      3. long flows a dummy in every column a flow skips reserves a slot,
                    so the flow never runs through a shape
      4. order      barycenter sweeps within each lane, fewest crossings kept
      5. rows       Brandes-Köpf: items align with their median predecessor
                    into straight blocks, packed to the top
      6. routing    orthogonal: horizontal runs in reserved slots, vertical
                    runs in the gaps between columns (or a free column);
                    back edges use the free channels between rows, and
                    runs that would overlap move onto parallel tracks
   ===================================================================== */

col_w(150).   row_h(120).   lane_hdr(30).   pool_hdr(30).   pool_gap(40).

node_size(T, 36, 36)  :- event_trigger(T, _, _), !.
node_size(gateway(_), 50, 50) :- !.
node_size(_, 100, 80).

diagrams_xml(Db, Def, PlaneOf, [Main | Subs]) :-
    main_layout(Db, Items),
    di_xml(Db, Def, PlaneOf, Items, Main),
    findall(D, ( member(node(S, subprocess, _, _), Db),
                 content(Db, S, 0, 0, [], Sh, Ed0, Ls, _, _, _),
                 separate(Ls, Ed0, Ed),
                 append(Sh, Ed, Is),
                 di_xml(Db, S, S, Is, D) ),
            Subs).

di_xml(Db, Base, Elem, Items,
       element('bpmndi:BPMNDiagram', [id=DId],
               [element('bpmndi:BPMNPlane', [id=PId, bpmnElement=Elem], Xs)])) :-
    cat([Base, '_diagram'], DId),
    cat([Base, '_plane'], PId),
    maplist(item_xml(Db), Items, Xs).

item_xml(Db, shape(Id, X, Y, W, H),
         element('bpmndi:BPMNShape', [id=DI, bpmnElement=Id | Extra],
                 [element('dc:Bounds', [x=X, y=Y, width=W, height=H], [])])) :-
    cat([Id, '_di'], DI),
    shape_extra(Db, Id, Extra).
item_xml(_, edge(F, Pts), element('bpmndi:BPMNEdge', [id=DI, bpmnElement=F], WPs)) :-
    cat([F, '_di'], DI),
    findall(element('di:waypoint', [x=X, y=Y], []), member(p(X, Y), Pts), WPs).

shape_extra(Db, Id, [isHorizontal=true]) :-
    ( member(pool(Id, _, _), Db) ; member(lane(Id, _, _, _), Db) ), !.
shape_extra(Db, Id, [isExpanded=false]) :- member(node(Id, subprocess, _, _), Db), !.
shape_extra(Db, Id, [isMarkerVisible=true]) :- member(node(Id, gateway(xor), _, _), Db), !.
shape_extra(_, _, []).

%  main_layout(+Db, -Items): shapes and edges of the top-level plane
main_layout(Db, Items) :-
    findall(P-N-Proc, member(pool(P, N, Proc), Db), Pools),
    Pools \== [], !,
    pool_hdr(PH), pool_gap(Gap),
    foldl(layout_pool(Db), Pools, Laid, 0-0, _-MaxW0),
    MaxW is max(MaxW0, 600),
    findall(I, ( member(L, Laid), pool_items(L, MaxW, PH, I0), member(I, I0) ), PoolItems),
    findall(P-s(0, Y, MaxW, H), member(laid(P, Y, H, _, _, _, _), Laid), PoolBoxes),
    findall(S, ( member(laid(_, _, _, _, Ss, _, _), Laid), member(S, Ss) ), NodeShapes),
    findall(E, ( member(mflow(M, S, T, _), Db),
                 mflow_edge(Db, PoolBoxes, NodeShapes, Gap, M, S, T, E) ), MEdges),
    findall(MY, ( member(_-s(_, Y, _, H), PoolBoxes),
                  ( MY is Y - Gap // 2 ; MY is Y + H + Gap // 2 ) ), MYs),
    findall(E, ( member(laid(_, _, _, _, _, Es, _), Laid), member(E, Es) ), SEdges),
    findall(X, ( member(laid(_, _, _, _, _, _, Xs-_), Laid), member(X, Xs) ), Xs),
    findall(Y, ( member(laid(_, _, _, _, _, _, _-Ys), Laid), member(Y, Ys) ), Ys0),
    append(Ys0, MYs, Ys),
    append(SEdges, MEdges, Edges0),
    separate(Xs-Ys, Edges0, Edges),
    append(PoolItems, Edges, Items).
main_layout(Db, Items) :-
    once(member(process(P, _), Db)),
    content(Db, P, 0, 0, [], Shapes, Edges0, Lines, _, _, _),
    separate(Lines, Edges0, Edges),
    append(Shapes, Edges, Items).

layout_pool(_, P-_-none, laid(P, Y, 80, [], [], [], []-[]), Y-W, Y1-W) :- !,
    pool_gap(G), Y1 is Y + 80 + G.
layout_pool(Db, P-_-Proc, laid(P, Y, H, Bands, Shapes, Edges, Lines), Y-W0, Y1-W) :-
    findall(lane(L, N, Proc, Ms), member(lane(L, N, Proc, Ms), Db), Lanes),
    pool_hdr(PH), lane_hdr(LH),
    ( Lanes = [] -> X0 = PH ; X0 is PH + LH ),
    content(Db, Proc, X0, Y, Lanes, Shapes, Edges, Lines, Bands, CW, H),
    W is max(W0, X0 + CW),
    pool_gap(G), Y1 is Y + H + G.

pool_items(laid(P, Y, H, Bands, Shapes, _, _), W, PH, [shape(P, 0, Y, W, H) | Is]) :-
    LW is W - PH,
    findall(shape(L, PH, LY, LW, LHt), ( member(band(L, LY, LHt), Bands), L \== none ), Ls),
    append(Ls, Shapes, Is).

%  content(+Db, +Container, +X0, +Y0, +Lanes,
%          -Shapes, -Edges, -Lines, -Bands, -Width, -Height)
%  Lines = GapXs-ChannelYs: where flows of the plane may need separating
content(Db, C, X0, Y0, Lanes, Shapes, Edges, Gaps-Channels, Bands, CW, H) :-
    findall(Id, ( member(node(Id, T, _, C), Db), T \= boundary(_, _, _) ), Ns),
    findall(f(F, S0, S, T), ( member(flow(F, S0, T, _, C), Db), anchor(Db, S0, S),
                              S \== T, memberchk(S, Ns), memberchk(T, Ns) ), Fs),
    findall(S-T, member(f(_, _, S, T), Fs), Es),
    back_edges(Ns, Es, Back),
    findall(E, ( member(E, Es), \+ memberchk(E, Back) ), Fwd),
    ranks(Ns, Fwd, RankA0),
    lane_assoc(Ns, Lanes, LaneA0),
    long_flows(Fs, Back, RankA0, LaneA0, Ds, Links, RankA, LaneA),
    append(Ns, Ds, Items),
    (   Ns = [] -> MaxR = 0
    ;   findall(R, ( member(N, Ns), get_assoc(N, RankA, R) ), Rs), list_max(Rs, MaxR)
    ),
    findall(Layer, ( between(0, MaxR, R),
                     findall((L-K)-I, ( nth0(K, Items, I), get_assoc(I, RankA, R),
                                        get_assoc(I, LaneA, L) ), Ks),
                     keysort(Ks, Ss), pairs_values(Ss, Layer) ),
            Layers0),
    order_layers(Layers0, Links, LaneA, Layers),
    rows(Layers, Links, LaneA, RowA),
    bands(Lanes, Items, LaneA, RowA, Y0, Bands, H),
    findall(S, ( member(N, Ns), node_shape(Db, N, X0, RankA, RowA, LaneA, Lanes, Bands, S) ),
            Shapes0),
    boundary_shapes(Db, C, Shapes0, BShapes),
    append(Shapes0, BShapes, Shapes),
    G = g(Db, X0, RankA, RowA, LaneA, Bands, Back, Shapes),
    findall(edge(F, Pts), ( member(flow(F, S, T, _, C), Db), route_flow(G, F, S, T, Pts) ),
            Edges),
    findall(X, ( between(0, MaxR, R), gap_x(G, R, X) ), Gaps),
    row_h(RH),
    findall(Y, ( member(band(_, BY, BH), Bands), Rows is (BH - 20) // RH, R1 is Rows - 1,
                 between(0, R1, R), member(DY, [10, 130]), Y is BY + DY + R * RH ),
            Channels),
    col_w(CWd), CW is (MaxR + 1) * CWd + 40.

anchor(Db, X, A) :-
    (   member(node(X, boundary(_, On, _), _, _), Db) -> A = On ; A = X ).

back_edges(Ns, Es, Back) :-
    findall(N, ( member(N, Ns), \+ member(_-N, Es) ), Roots),
    append(Roots, Ns, Order),
    dfs_all(Order, Es, [], _, [], Back).

dfs_all([], _, V, V, B, B).
dfs_all([N|Ns], Es, V0, V, B0, B) :-
    (   memberchk(N, V0) -> V1 = V0, B1 = B0
    ;   dfs(N, [N], Es, [N|V0], V1, B0, B1)
    ),
    dfs_all(Ns, Es, V1, V, B1, B).

dfs(N, Stack, Es, V0, V, B0, B) :-
    findall(M, member(N-M, Es), Ms),
    dfs_succ(Ms, N, Stack, Es, V0, V, B0, B).

dfs_succ([], _, _, _, V, V, B, B).
dfs_succ([M|Ms], N, Stack, Es, V0, V, B0, B) :-
    (   memberchk(M, Stack) -> V1 = V0, B1 = [N-M|B0]
    ;   memberchk(M, V0)    -> V1 = V0, B1 = B0
    ;   dfs(M, [M|Stack], Es, [M|V0], V1, B0, B1)
    ),
    dfs_succ(Ms, N, Stack, Es, V1, V, B1, B).

%  longest-path ranks on the acyclic forward edges
ranks(Ns, Fwd, A) :-
    findall(N-0, member(N, Ns), Ps),
    list_to_assoc(Ps, A0),
    length(Ns, K),
    relax(K, Fwd, A0, A).

relax(0, _, A, A) :- !.
relax(K, Fwd, A0, A) :-
    foldl(relax_edge, Fwd, A0, A1),
    K1 is K - 1,
    (   A1 == A0 -> A = A1 ; relax(K1, Fwd, A1, A) ).

relax_edge(S-T, A0, A) :-
    get_assoc(S, A0, RS), get_assoc(T, A0, RT),
    R is max(RT, RS + 1),
    put_assoc(T, A0, R, A).

lane_assoc(Ns, Lanes, A) :-
    findall(N-L, ( member(N, Ns), lane_index(Lanes, N, L) ), Ps),
    list_to_assoc(Ps, A).

lane_index(Lanes, N, L) :-
    (   nth0(L, Lanes, lane(_, _, _, Ms)), memberchk(N, Ms) -> true ; L = 0 ).

%  long_flows(+Flows, +Back, +Ranks0, +Lanes0, -Dummies, -Links, -Ranks, -Lanes)
%  Splits every flow into links l(U, V, Bias) between adjacent columns, with a
%  dummy d(Flow, Rank) in the lane of its source for every column it skips.
%  Back edges get neither: they run through the free channels between rows.
%  Bias 1 marks a flow leaving a boundary event: its target prefers the row
%  below the host.
long_flows([], _, R, L, [], [], R, L).
long_flows([f(F, S0, S, T)|Fs], Back, R0, L0, Ds, Links, R, L) :-
    (   memberchk(S-T, Back)
    ->  Ds1 = [], Links1 = [], R1 = R0, L1 = L0
    ;   ( S0 == S -> B = 0 ; B = 1 ),
        get_assoc(S, R0, RS), get_assoc(T, R0, RT), get_assoc(S, L0, Lane),
        K0 is RS + 1, K1 is RT - 1,
        findall(d(F, K), between(K0, K1, K), Ds1),
        foldl(dummy(Lane), Ds1, R0-L0, R1-L1),
        append([S|Ds1], [T], Chain),
        chain_links(Chain, B, Links1)
    ),
    long_flows(Fs, Back, R1, L1, Ds2, Links2, R, L),
    append(Ds1, Ds2, Ds), append(Links1, Links2, Links).

dummy(Lane, d(F, K), R0-L0, R-L) :-
    put_assoc(d(F, K), R0, K, R),
    put_assoc(d(F, K), L0, Lane, L).

chain_links([_], _, []).
chain_links([U, V|Vs], B, [l(U, V, B)|Ls]) :- chain_links([V|Vs], 0, Ls).

%  order_layers(+Layers0, +Links, +LaneA, -Layers): barycenter sweeps (down,
%  then up) while they reduce the crossings; ties keep the declaration order
order_layers(Ls0, Links, LaneA, Ls) :-
    crossings(Ls0, Links, K0),
    improve(4, Links, LaneA, Ls0, K0, Ls).

improve(N, Links, LaneA, Ls0, K0, Ls) :-
    (   N > 0, K0 > 0,
        sweep(down, Links, LaneA, Ls0, Ls1),
        reverse(Ls1, Rs1), sweep(up, Links, LaneA, Rs1, Rs2), reverse(Rs2, Ls2),
        crossings(Ls1, Links, K1), crossings(Ls2, Links, K2),
        ( K2 =< K1 -> Ls3 = Ls2, K3 = K2 ; Ls3 = Ls1, K3 = K1 ),
        K3 < K0
    ->  N1 is N - 1, improve(N1, Links, LaneA, Ls3, K3, Ls)
    ;   Ls = Ls0
    ).

sweep(Dir, Links, LaneA, [L|Ls0], [L|Ls]) :-
    foldl(reorder(Dir, Links, LaneA), Ls0, Ls, L, _).

reorder(Dir, Links, LaneA, Layer0, Layer, Prev, Layer) :-
    positions(Prev, PA),
    findall((Ln-B-K)-V, ( nth0(K, Layer0, V), get_assoc(V, LaneA, Ln),
                          bary(Dir, Links, PA, K, V, B) ), Ks),
    keysort(Ks, Ss), pairs_values(Ss, Layer).

positions(Layer, PA) :-
    findall(I-K, nth0(K, Layer, I), Ps),
    list_to_assoc(Ps, PA).

%  bary(+Dir, +Links, +Positions, +Pos, +Item, -B): 1000 x mean neighbour position
bary(Dir, Links, PA, K, V, B) :-
    findall(P, ( neighbour(Dir, Links, V, U, Bias), get_assoc(U, PA, P0),
                 P is P0 * 1000 + Bias ), Ps),
    (   Ps = [] -> B is K * 1000
    ;   sum_list(Ps, S), length(Ps, N), B is S // N
    ).

neighbour(down, Links, V, U, Bias) :- member(l(U, V, B), Links), Bias is B * 500.
neighbour(up, Links, V, W, Bias)   :- member(l(V, W, B), Links), Bias is -B * 500.

crossings([], _, 0).
crossings([_], _, 0).
crossings([A, B|Ls], Links, K) :-
    positions(A, PA), positions(B, PB),
    findall(P-Q, ( member(l(U, V, _), Links), get_assoc(U, PA, P), get_assoc(V, PB, Q) ),
            Es),
    findall(x, ( append(_, [P1-Q1|Rest], Es), member(P2-Q2, Rest),
                 (P1 - P2) * (Q1 - Q2) < 0 ), Xs),
    length(Xs, K1),
    crossings([B|Ls], Links, K2),
    K is K1 + K2.

%  rows(+Layers, +Links, +LaneA, -RowA): Brandes-Köpf, top-left alignment.
%  Left to right, each item joins the block of the median of its neighbours in
%  the column before (same lane, order preserved), so chains run straight; a
%  link crossing a dummy-to-dummy link is never aligned, so long flows stay
%  straight first. Blocks are then packed to the top, one row apart.
rows(Layers, Links, LaneA, RowA) :-
    findall(V-K, ( member(Layer, Layers), nth0(K, Layer, V) ), Ps),
    list_to_assoc(Ps, PosA),
    findall(V-V, member(V-_, Ps), Rs0),
    list_to_assoc(Rs0, RootA0),
    foldl(align_layer(Links, LaneA, PosA), Layers, RootA0, RootA),
    findall(R, member(_-R, Rs0), Items),
    findall(R, ( member(V, Items), get_assoc(V, RootA, R) ), Roots0),
    sort(Roots0, Roots),
    findall(R1-R2, ( member(Layer, Layers), append(_, [V1, V2|_], Layer),
                     get_assoc(V1, LaneA, L), get_assoc(V2, LaneA, L),
                     get_assoc(V1, RootA, R1), get_assoc(V2, RootA, R2) ),
            Below),
    ranks(Roots, Below, YA),
    findall(V-Y, ( member(V, Items), get_assoc(V, RootA, R), get_assoc(R, YA, Y) ), Ys),
    list_to_assoc(Ys, RowA).

align_layer(Links, LaneA, PosA, Layer, RA0, RA) :-
    conflicts(Links, PosA, Layer, Marked),
    foldl(align_item(Links, LaneA, PosA, Marked), Layer, RA0-(-1), RA-_).

align_item(Links, LaneA, PosA, Marked, V, RA0-R0, RA-R) :-
    get_assoc(V, LaneA, L),
    findall(P-U, ( member(l(U, V, 0), Links), get_assoc(U, LaneA, L),
                   get_assoc(U, PosA, P) ), Us0),
    keysort(Us0, Us),
    length(Us, D), Lo is (D - 1) // 2, Hi is D // 2,
    (   member(M, [Lo, Hi]), nth0(M, Us, P-U),
        \+ memberchk(U-V, Marked), R0 < P
    ->  get_assoc(U, RA0, Root), put_assoc(V, RA0, Root, RA), R = P
    ;   RA = RA0, R = R0
    ).

%  conflicts(+Links, +PosA, +Layer, -Marked): links into Layer that cross a
%  link between two dummies
conflicts(Links, PosA, Layer, Marked) :-
    findall(U-V, ( member(V, Layer), member(l(U, V, _), Links) ), In),
    findall(U-V, ( member(U-V, In), U = d(_, _), V = d(_, _) ), Inner),
    findall(U-V, ( member(U-V, In), \+ ( U = d(_, _), V = d(_, _) ),
                   member(U2-V2, Inner),
                   get_assoc(U, PosA, P1), get_assoc(U2, PosA, P2),
                   get_assoc(V, PosA, Q1), get_assoc(V2, PosA, Q2),
                   (P1 - P2) * (Q1 - Q2) < 0 ),
            Marked).

%  bands(+Lanes, ...): one horizontal band per lane (or one band if no lanes)
bands(Lanes, Ns, LaneA, RowA, Y0, Bands, H) :-
    (   Lanes = [] -> Ids = [none] ; findall(L, member(lane(L, _, _, _), Lanes), Ids) ),
    row_h(RH),
    foldl(band(Ns, LaneA, RowA, RH), Ids, Bands, 0-Y0, _-Y1),
    H is Y1 - Y0.

band(Ns, LaneA, RowA, RH, LaneId, band(LaneId, Y, BH), I-Y, I1-Y1) :-
    findall(R, ( member(N, Ns), get_assoc(N, LaneA, I), get_assoc(N, RowA, R) ), Rs),
    ( Rs = [] -> Rows = 1 ; list_max(Rs, M), Rows is M + 1 ),
    BH is Rows * RH + 20,
    Y1 is Y + BH, I1 is I + 1.

node_shape(Db, N, X0, RankA, RowA, LaneA, _, Bands, shape(N, X, Y, W, H)) :-
    member(node(N, T, _, _), Db), !,
    node_size(T, W, H),
    get_assoc(N, RankA, R), get_assoc(N, RowA, Row), get_assoc(N, LaneA, L),
    nth0(L, Bands, band(_, BY, _)),
    col_w(CW), row_h(RH),
    CX is X0 + 70 + R * CW,
    CY is BY + 70 + Row * RH,
    X is CX - W // 2, Y is CY - H // 2.

boundary_shapes(Db, C, Shapes, BShapes) :-
    findall(B-On, member(node(B, boundary(_, On, _), _, C), Db), Bs),
    findall(shape(B, X, Y, 36, 36),
            ( member(B-On, Bs),
              memberchk(shape(On, AX, AY, AW, AH), Shapes),
              findall(B1, member(B1-On, Bs), Siblings),
              nth0(K, Siblings, B),
              X is AX + AW - 38 - K * 40, Y is AY + AH - 18 ),
            BShapes).

%  route_flow(+G, +Flow, +Source, +Target, -Points)
%  Tries the candidate routes in order of preference and takes the first that
%  crosses no shape; the last candidate only uses reserved slots and gaps.
route_flow(G, F, S0, T, Pts) :-
    G = g(Db, _, RankA, _, _, _, _, Shapes),
    memberchk(shape(S0, SX, SY, SW, SH), Shapes),
    memberchk(shape(T, TX, TY, TW, TH), Shapes),
    anchor(Db, S0, S),
    get_assoc(S, RankA, RS), get_assoc(T, RankA, RT),
    SBox = s(SX, SY, SW, SH), TBox = s(TX, TY, TW, TH),
    (   S == T
    ->  loop_route(G, S0, S, RS, SBox, TBox, Pts)
    ;   findall(Ps, candidate(G, F, S0, S, T, RS, RT, SBox, TBox, Ps), Cands),
        (   member(Pts, Cands), clear(Shapes, [S0, S, T], Pts) -> true
        ;   append(_, [Pts], Cands)
        )
    ).

candidate(G, F, S0, S, T, RS, RT, SBox, TBox, Pts) :-
    G = g(Db, _, _, _, _, _, Back, _),
    (   memberchk(S-T, Back)
    ->  gap_x(G, RT, X), Gaps = [X],
        back_ends(G, F, S0, S, T, SBox, TBox, Start, Levels, End)
    ;   K0 is RS + 1, K1 is RT - 1,
        findall(Y, ( between(K0, K1, K), dummy_y(G, d(F, K), Y) ), Ds),
        findall(X, ( between(RS, K1, C), gap_x(G, C, X) ), Gaps),
        forward_ends(Db, S0, S, T, SBox, TBox, Ds, Start, Levels, End)
    ),
    path(Start, Levels, Gaps, End, Pts0),
    simplify(Pts0, Pts).

%  forward_ends(..., -Start, -Levels, -End): Levels holds the y of the flow in
%  each column; a node with several outgoing (incoming) flows may use its
%  bottom or top side, all others leave right and enter left
forward_ends(Db, S0, S, T, s(SX, SY, SW, SH), s(TX, TY, TW, TH), Ds, Start, Levels, End) :-
    SCX is SX + SW // 2, SCY is SY + SH // 2, SB is SY + SH,
    TCX is TX + TW // 2, TCY is TY + TH // 2,
    append(Ds, [TCY], [L1|Ls]),
    (   S0 \== S                                    % boundary event: leave below
    ->  Start = [p(SCX, SB), p(SCX, L)],
        (   L1 > SB, L = L1 ; L is SB + 12 ),
        Levels0 = [L, L1|Ls]
    ;   fans_out(Db, S0), L1 =\= SCY
    ->  ( L1 > SCY -> Port = SB ; Port = SY ),
        (   Start = [p(SCX, Port), p(SCX, L1)], Levels0 = [L1, L1|Ls]
        ;   SR is SX + SW, Start = [p(SR, SCY)], Levels0 = [SCY, L1|Ls]
        )
    ;   SR is SX + SW, Start = [p(SR, SCY)], Levels0 = [SCY, L1|Ls]
    ),
    append(Init, [Prev, TCY], Levels0),
    (   fans_in(Db, T), Prev =\= TCY
    ->  ( Prev > TCY -> Port1 is TY + TH ; Port1 = TY ),
        (   append(Init, [Prev, Prev], Levels), End = [p(TCX, Prev), p(TCX, Port1)]
        ;   Levels = Levels0, End = [p(TX, TCY)]
        )
    ;   Levels = Levels0, End = [p(TX, TCY)]
    ).

%  back edges leave the source below (or above), run left through the free
%  channel between rows, change channels in the gap right of the target and
%  enter it from below (or from above, when that side is taken); on an
%  activity they keep off the centre, which message flows and branches use
back_ends(G, F, S0, S, T, s(SX, SY, SW, SH), s(TX, TY, TW, TH), Start, [LS, LT], End) :-
    G = g(Db, _, _, _, _, _, _, _),
    SCY is SY + SH // 2, SB is SY + SH,
    TCY is TY + TH // 2, TB is TY + TH,
    ( is_activity(Db, S0) -> SCX is SX + SW // 4 ; SCX is SX + SW // 2 ),
    ( is_activity(Db, T) -> TCX is TX + 3 * TW // 4 ; TCX is TX + TW // 2 ),
    row_h(RH), Half is RH // 2,
    (   S0 \== S -> LS is SB + 12, SPort = SB
    ;   sides(G, F, S0, SDirs),
        member(SDir, SDirs),
        (   SDir == below -> LS is SCY + Half, SPort = SB
        ;   LS is SCY - Half, SPort = SY
        )
    ),
    sides(G, F, T, TDirs),
    member(TDir, TDirs),
    (   TDir == below -> LT is TCY + Half, TPort = TB
    ;   LT is TCY - Half, TPort = TY
    ),
    Start = [p(SCX, SPort), p(SCX, LS)],
    End = [p(TCX, LT), p(TCX, TPort)].

loop_route(G, S0, S, R, s(SX, SY, SW, SH), s(TX, TY, TW, TH), Pts) :-
    SCX is SX + SW // 2, SCY is SY + SH // 2, SB is SY + SH,
    TCX is TX + TW // 2, TB is TY + TH,
    (   S0 \== S
    ->  L is SB + 12,
        Pts = [p(SCX, SB), p(SCX, L), p(TCX, L), p(TCX, TB)]
    ;   gap_x(G, R, GX), SR is SX + SW, row_h(RH), L is SCY + RH // 2,
        Pts = [p(SR, SCY), p(GX, SCY), p(GX, L), p(SCX, L), p(SCX, SB)]
    ).

is_activity(Db, Id) :- member(node(Id, T, _, _), Db), !, activity(T).

fans_out(Db, Id) :-
    (   member(node(Id, gateway(_), _, _), Db) -> true
    ;   findall(F, member(flow(F, Id, _, _, _), Db), [_, _|_])
    ).

fans_in(Db, Id) :-
    (   member(node(Id, gateway(_), _, _), Db) -> true
    ;   findall(F, member(flow(F, _, Id, _, _), Db), [_, _|_])
    ).

%  sides(+G, +Flow, +Node, -Sides): below and above, the free ones first
sides(G, F, N, Sides) :-
    findall(D, ( member(D, [below, above]), \+ busy(G, F, N, D) ), Free),
    findall(D, ( member(D, [below, above]), busy(G, F, N, D) ), Taken),
    append(Free, Taken, Sides).

%  busy(+G, +Flow, +Node, ?Side): another flow probably uses that side of
%  Node: a message flow (pools are stacked in the order they are declared),
%  or a branch to or from another row of a node that fans out (in)
busy(g(Db, _, _, _, _, _, _, Shapes), F, N, Side) :-
    (   ( member(mflow(_, N, O, _), Db) ; member(mflow(_, O, N, _), Db) ),
        pool_of(Db, N, P), pool_of(Db, O, PO),
        findall(X, member(pool(X, _, _), Db), Ps),
        nth0(I, Ps, P), nth0(IO, Ps, PO), IO =\= I,
        ( IO > I -> Side = below ; Side = above )
    ;   (   fans_out(Db, N), member(flow(F1, N, O, _, _), Db)
        ;   fans_in(Db, N), member(flow(F1, O, N, _, _), Db)
        ),
        F1 \== F,
        memberchk(shape(N, _, NY, _, NH), Shapes), memberchk(shape(O, _, OY, _, OH), Shapes),
        NC is NY + NH // 2, OC is OY + OH // 2, OC =\= NC,
        ( OC > NC -> Side = below ; Side = above )
    ), !.

%  path(+Start, +Levels, +Gaps, +End, -Points): the flow runs at Levels[i] in
%  column i and changes level in the gap Gaps[i] to the right of it
path(Start, Levels, Gaps, End, Pts) :-
    jogs(Gaps, Levels, Mid),
    append([Start, Mid, End], Pts).

jogs([], _, []).
jogs([X|Xs], [L1, L2|Ls], [p(X, L1), p(X, L2)|Ps]) :- jogs(Xs, [L2|Ls], Ps).

simplify([P, P|Ps], Qs) :- !, simplify([P|Ps], Qs).
simplify([p(X1, Y1), p(X2, Y2), p(X3, Y3)|Ps], Qs) :-
    ( X1 =:= X2, X2 =:= X3 ; Y1 =:= Y2, Y2 =:= Y3 ), !,
    simplify([p(X1, Y1), p(X3, Y3)|Ps], Qs).
simplify([], []).
simplify([P|Ps], [P|Qs]) :- simplify(Ps, Qs).

%  clear(+Shapes, +Except, +Points): no segment runs through another shape
clear(Shapes, Except, Pts) :-
    \+ ( append(_, [p(X1, Y1), p(X2, Y2)|_], Pts),
         member(shape(Id, X, Y, W, H), Shapes), \+ memberchk(Id, Except),
         min(X1, X2) < X + W - 1, max(X1, X2) > X + 1,
         min(Y1, Y2) < Y + H - 1, max(Y1, Y2) > Y + 1 ).

dummy_y(g(_, _, _, RowA, LaneA, Bands, _, _), D, Y) :-
    get_assoc(D, RowA, Row), get_assoc(D, LaneA, L),
    nth0(L, Bands, band(_, BY, _)),
    row_h(RH), Y is BY + 70 + Row * RH.

gap_x(g(_, X0, _, _, _, _, _, _), C, X) :-
    col_w(CW), X is X0 + 70 + C * CW + CW // 2.

%  separate(+Xs-Ys, +Edges0, -Edges): segments of different edges that overlap
%  on the same line (a gap's x or a channel's y) are moved onto parallel tracks
separate(Xs-Ys, Es0, Es) :-
    separate(v, Xs, Es0, Es1),
    separate(h, Ys, Es1, Es).

separate(O, Lines, Es0, Es) :-
    findall(s(L, Lo, Hi, K, I), ( nth0(K, Es0, edge(_, Pts)), nth0(I, Pts, P1),
                                  I1 is I + 1, nth0(I1, Pts, P2),
                                  on_line(O, P1, P2, L, Lo, Hi), memberchk(L, Lines) ),
            Segs),
    foldl(track, Segs, [], Tracks),
    findall(edge(F, Pts), ( nth0(K, Es0, edge(F, Pts0)),
                            findall(Q, ( nth0(J, Pts0, P), shifted(O, Tracks, K, J, P, Q) ),
                                    Pts) ),
            Es).

on_line(v, p(X, Y1), p(X, Y2), X, Lo, Hi) :- Y1 =\= Y2, Lo is min(Y1, Y2), Hi is max(Y1, Y2).
on_line(h, p(X1, Y), p(X2, Y), Y, Lo, Hi) :- X1 =\= X2, Lo is min(X1, X2), Hi is max(X1, X2).

track(s(L, Lo, Hi, K, I), Ts, [t(K, I, D, L, Lo, Hi)|Ts]) :-
    (   member(D, [0, 8, -8, 16, -16]),
        \+ ( member(t(K2, _, D, L, Lo2, Hi2), Ts), K2 \== K, Lo < Hi2, Lo2 < Hi )
    ->  true
    ;   D = 0
    ).

shifted(O, Ts, K, J, p(X, Y), P) :-
    J0 is J - 1,
    (   ( memberchk(t(K, J, D, _, _, _), Ts) ; memberchk(t(K, J0, D, _, _, _), Ts) ), D =\= 0
    ->  ( O == v -> X1 is X + D, P = p(X1, Y) ; Y1 is Y + D, P = p(X, Y1) )
    ;   P = p(X, Y)
    ).

%  message flows: vertical between pools, through the gap below/above the
%  source pool; an end blocked by other shapes leaves through the channel
%  beside its row and the gap next to it
mflow_edge(Db, PoolBoxes, NodeShapes, Gap, M, S, T, edge(M, Pts)) :-
    end_box(Db, PoolBoxes, NodeShapes, S, SId, SKind, s(SX, SY, SW, SH)),
    end_box(Db, PoolBoxes, NodeShapes, T, TId, TKind, s(TX, TY, TW, TH)),
    pool_of(Db, S, SP), memberchk(SP-s(_, SPY, _, SPH), PoolBoxes),
    SCX is SX + SW // 2, TCX is TX + TW // 2,
    (   SY < TY
    ->  Y1 is SY + SH, Y2 = TY, MY is SPY + SPH + Gap // 2, Dir = 1
    ;   Y1 = SY, Y2 is TY + TH, MY is SPY - Gap // 2, Dir = -1
    ),
    Back is -Dir,
    findall(Ps, ( mf_legs(SKind-SCX-Y1-SH-Dir, TKind-TCX-Y2-TH-Back, MY, Out, In),
                  reverse(In, NI), append(Out, NI, Ps0), simplify(Ps0, Ps) ),
            [First|Cands]),
    (   member(Pts, [First|Cands]), clear(NodeShapes, [SId, TId], Pts) -> true
    ;   Pts = First
    ).

%  mf_legs(+Source, +Target, +MY, -Out, -In): both legs; a pool end simply
%  continues the other leg straight on
mf_legs(pool-_-Y1-_-_, TK-X2-Y2-H2-D2, MY, [p(X, Y1), p(X, MY)], In) :-
    TK \== pool, !,
    mf_leg(TK, X2, Y2, H2, D2, MY, In), append(_, [p(X, MY)], In).
mf_legs(SK-X1-Y1-H1-D1, pool-_-Y2-_-_, MY, Out, [p(X, Y2), p(X, MY)]) :-
    SK \== pool, !,
    mf_leg(SK, X1, Y1, H1, D1, MY, Out), append(_, [p(X, MY)], Out).
mf_legs(SK-X1-Y1-H1-D1, TK-X2-Y2-H2-D2, MY, Out, In) :-
    mf_leg(SK, X1, Y1, H1, D1, MY, Out),
    mf_leg(TK, X2, Y2, H2, D2, MY, In).

%  mf_leg(+Kind, +X, +Port, +Height, +Dir, +MY, -Points): from the port to the
%  pool gap at MY, straight or through the channel and the gap left or right
mf_leg(_, X, Y, _, _, MY, [p(X, Y), p(X, MY)]).
mf_leg(node, X, Y, H, Dir, MY, [p(X, Y), p(X, YC), p(GX, YC), p(GX, MY)]) :-
    row_h(RH), col_w(CW),
    YC is Y + Dir * ((RH - H) // 2),
    member(Side, [1, -1]),
    GX is X + Side * (CW // 2).

end_box(_, PoolBoxes, _, X, X, pool, B) :- memberchk(X-B, PoolBoxes), !.
end_box(_, _, Shapes, X, X, node, s(SX, SY, W, H)) :-
    memberchk(shape(X, SX, SY, W, H), Shapes), !.
end_box(Db, PoolBoxes, Shapes, X, Id, K, B) :-  % inside a collapsed subprocess
    member(node(X, _, _, C), Db),
    end_box(Db, PoolBoxes, Shapes, C, Id, K, B).

/* =====================================================================
   5. XML term -> text  (a DCG; no string concatenation anywhere)
   ===================================================================== */

bpmn_chars(Model, Cs) :-
    bpmn_xml(Model, X),
    phrase(xml_document(X), Cs).

bpmn_write(Model, File) :-
    bpmn_xml(Model, X),
    phrase_to_file(xml_document(X), File).

xml_document(E) -->
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
    xml(E, 0).

xml(element(N, As, Cs), I) -->
    indent(I), "<", value(N), attributes(As),
    (   { Cs == [] }
    ->  "/>\n"
    ;   { Cs = [T], T \= element(_, _, _) }
    ->  ">", { value_chars(T, TCs) }, escaped(TCs, text), "</", value(N), ">\n"
    ;   ">\n", { I1 is I + 2 }, xml_list(Cs, I1), indent(I), "</", value(N), ">\n"
    ).

xml_list([], _) --> [].
xml_list([E|Es], I) --> xml(E, I), xml_list(Es, I).

attributes([]) --> [].
attributes([K=V|As]) -->
    (   { V == none } -> []
    ;   " ", value(K), "=\"", { value_chars(V, Cs) }, escaped(Cs, attr), "\""
    ),
    attributes(As).

value(V) --> { value_chars(V, Cs) }, seq(Cs).

value_chars(V, Cs) :-
    (   V == []   -> Cs = []
    ;   atom(V)   -> atom_chars(V, Cs)
    ;   number(V) -> number_chars(V, Cs)
    ;   Cs = V                                       % already a list of chars
    ).

escaped([], _) --> [].
escaped([C|Cs], Ctx) --> esc(C, Ctx), escaped(Cs, Ctx).

esc('&', _)     --> !, "&amp;".
esc('<', _)     --> !, "&lt;".
esc('>', _)     --> !, "&gt;".
esc('"', attr)  --> !, "&quot;".
esc('\n', attr) --> !, "&#10;".
esc(C, _)       --> [C].

indent(0) --> !.
indent(N) --> " ", { N1 is N - 1 }, indent(N1).

/* ===================================================================== */

cat(Parts, Atom) :-
    maplist(value_chars, Parts, Css),
    append(Css, Cs),
    atom_chars(Atom, Cs).
