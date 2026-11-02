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
   4. Diagram interchange: a small layered auto-layout
      columns = longest path rank, rows = first free row near predecessor
   ===================================================================== */

col_w(150).   row_h(120).   lane_hdr(30).   pool_hdr(30).   pool_gap(40).

node_size(T, 36, 36)  :- event_trigger(T, _, _), !.
node_size(gateway(_), 50, 50) :- !.
node_size(_, 100, 80).

diagrams_xml(Db, Def, PlaneOf, [Main | Subs]) :-
    main_layout(Db, Items),
    di_xml(Db, Def, PlaneOf, Items, Main),
    findall(D, ( member(node(S, subprocess, _, _), Db),
                 content(Db, S, 0, 0, [], Sh, Ed, _, _, _),
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
    findall(P-s(0, Y, MaxW, H), member(laid(P, Y, H, _, _, _), Laid), PoolBoxes),
    findall(S, ( member(laid(_, _, _, _, Ss, _), Laid), member(S, Ss) ), NodeShapes),
    findall(E, ( member(mflow(M, S, T, _), Db),
                 mflow_edge(Db, PoolBoxes, NodeShapes, Gap, M, S, T, E) ), MEdges),
    append(PoolItems, MEdges, Items).
main_layout(Db, Items) :-
    once(member(process(P, _), Db)),
    content(Db, P, 0, 0, [], Shapes, Edges, _, _, _),
    append(Shapes, Edges, Items).

layout_pool(_, P-_-none, laid(P, Y, 80, [], [], []), Y-W, Y1-W) :- !,
    pool_gap(G), Y1 is Y + 80 + G.
layout_pool(Db, P-_-Proc, laid(P, Y, H, Bands, Shapes, Edges), Y-W0, Y1-W) :-
    findall(lane(L, N, Proc, Ms), member(lane(L, N, Proc, Ms), Db), Lanes),
    pool_hdr(PH), lane_hdr(LH),
    ( Lanes = [] -> X0 = PH ; X0 is PH + LH ),
    content(Db, Proc, X0, Y, Lanes, Shapes, Edges, Bands, CW, H),
    W is max(W0, X0 + CW),
    pool_gap(G), Y1 is Y + H + G.

pool_items(laid(P, Y, H, Bands, Shapes, Edges), W, PH, [shape(P, 0, Y, W, H) | Is]) :-
    LW is W - PH,
    findall(shape(L, PH, LY, LW, LHt), ( member(band(L, LY, LHt), Bands), L \== none ), Ls),
    append([Ls, Shapes, Edges], Is).

%  content(+Db, +Container, +X0, +Y0, +Lanes, -Shapes, -Edges, -Bands, -Width, -Height)
content(Db, C, X0, Y0, Lanes, Shapes, Edges, Bands, CW, H) :-
    findall(Id, ( member(node(Id, T, _, C), Db), T \= boundary(_, _, _) ), Ns),
    findall(S-T, ( member(flow(_, S0, T, _, C), Db), anchor(Db, S0, S),
                   S \== T, memberchk(S, Ns), memberchk(T, Ns) ), Es),
    findall(S-T, ( member(flow(_, S0, T, _, C), Db),
                   member(node(S0, boundary(_, S, _), _, _), Db) ), BEs),
    back_edges(Ns, Es, Back),
    findall(E, ( member(E, Es), \+ memberchk(E, Back) ), Fwd),
    ranks(Ns, Fwd, RankA),
    lane_assoc(Ns, Lanes, LaneA),
    findall((R-I)-N, ( nth0(I, Ns, N), get_assoc(N, RankA, R) ), Keyed),
    keysort(Keyed, Sorted), pairs_values(Sorted, Ordered),
    empty_assoc(RA0),
    assign_rows(Ordered, Fwd, BEs, LaneA, RankA, [], RA0, RowA),
    bands(Lanes, Ns, LaneA, RowA, Y0, Bands, H),
    findall(S, ( member(N, Ns), node_shape(Db, N, X0, RankA, RowA, LaneA, Lanes, Bands, S) ),
            Shapes0),
    boundary_shapes(Db, C, Shapes0, BShapes),
    append(Shapes0, BShapes, Shapes),
    findall(edge(F, Pts), ( member(flow(F, S, T, _, C), Db),
                            route_flow(Db, Shapes, S, T, Pts) ), Edges),
    (   Ns = [] -> MaxR = 0
    ;   findall(R, ( member(N, Ns), get_assoc(N, RankA, R) ), Rs), list_max(Rs, MaxR)
    ),
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

assign_rows([], _, _, _, _, _, RA, RA).
assign_rows([N|Ns], Fwd, BEs, LaneA, RankA, Occ, RA0, RA) :-
    get_assoc(N, LaneA, L), get_assoc(N, RankA, R),
    findall(PR, ( member(P-N, Fwd), get_assoc(P, LaneA, L), get_assoc(P, RA0, PR0),
                  ( memberchk(P-N, BEs) -> PR is PR0 + 1 ; PR = PR0 ) ), PRs),
    ( PRs = [] -> Pref = 0 ; list_min(PRs, Pref) ),
    free_row(Pref, L, R, Occ, Row),
    put_assoc(N, RA0, Row, RA1),
    assign_rows(Ns, Fwd, BEs, LaneA, RankA, [o(L, R, Row)|Occ], RA1, RA).

free_row(P, L, R, Occ, Row) :-
    (   memberchk(o(L, R, P), Occ) -> P1 is P + 1, free_row(P1, L, R, Occ, Row)
    ;   Row = P
    ).

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

route_flow(Db, Shapes, S, T, Pts) :-
    memberchk(shape(S, SX, SY, SW, SH), Shapes),
    memberchk(shape(T, TX, TY, TW, TH), Shapes),
    shape_kind(Db, S, SK), shape_kind(Db, T, TK),
    route(SK, s(SX, SY, SW, SH), TK, s(TX, TY, TW, TH), Pts).

shape_kind(Db, Id, K) :-
    member(node(Id, T, _, _), Db), !,
    (   T = gateway(_)        -> K = gateway
    ;   T = boundary(_, _, _) -> K = boundary
    ;   K = other
    ).

route(SK, s(SX, SY, SW, SH), TK, s(TX, TY, TW, TH), Pts) :-
    SCX is SX + SW // 2, SCY is SY + SH // 2,
    TCX is TX + TW // 2, TCY is TY + TH // 2,
    SR is SX + SW, SB is SY + SH,
    (   SK == boundary
    ->  (   TCY > SB, TX > SCX
        ->  Pts = [p(SCX, SB), p(SCX, TCY), p(TX, TCY)]
        ;   Y1 is max(SB, TY + TH) + 20, XL is TX - 20,
            Pts = [p(SCX, SB), p(SCX, Y1), p(XL, Y1), p(XL, TCY), p(TX, TCY)]
        )
    ;   TCX =< SCX                                  % back edge: go round below
    ->  TB is TY + TH, Y1 is max(SB, TB) + 20,
        Pts = [p(SCX, SB), p(SCX, Y1), p(TCX, Y1), p(TCX, TB)]
    ;   SCY =:= TCY
    ->  Pts = [p(SR, SCY), p(TX, TCY)]
    ;   SK == gateway                               % split: leave top/bottom
    ->  ( TCY > SCY -> GY = SB ; GY = SY ),
        Pts = [p(SCX, GY), p(SCX, TCY), p(TX, TCY)]
    ;   TK == gateway                               % join: enter top/bottom
    ->  ( SCY < TCY -> GY = TY ; GY is TY + TH ),
        Pts = [p(SR, SCY), p(TCX, SCY), p(TCX, GY)]
    ;   MX is (SR + TX) // 2,
        Pts = [p(SR, SCY), p(MX, SCY), p(MX, TCY), p(TX, TCY)]
    ).

%  message flows: vertical between pools, through the gap below/above the source pool
mflow_edge(Db, PoolBoxes, NodeShapes, Gap, M, S, T, edge(M, Pts)) :-
    end_box(Db, PoolBoxes, NodeShapes, S, SKind, s(SX, SY, SW, SH)),
    end_box(Db, PoolBoxes, NodeShapes, T, TKind, s(TX, TY, TW, TH)),
    pool_of(Db, S, SP), memberchk(SP-s(_, SPY, _, SPH), PoolBoxes),
    SCX0 is SX + SW // 2, TCX0 is TX + TW // 2,
    (   SKind == pool, TKind \== pool -> SCX = TCX0, TCX = TCX0
    ;   TKind == pool, SKind \== pool -> SCX = SCX0, TCX = SCX0
    ;   SCX = SCX0, TCX = TCX0
    ),
    (   SY < TY
    ->  Y1 is SY + SH, Y2 = TY, MY is SPY + SPH + Gap // 2
    ;   Y1 = SY, Y2 is TY + TH, MY is SPY - Gap // 2
    ),
    (   SCX =:= TCX
    ->  Pts = [p(SCX, Y1), p(TCX, Y2)]
    ;   Pts = [p(SCX, Y1), p(SCX, MY), p(TCX, MY), p(TCX, Y2)]
    ).

end_box(_, PoolBoxes, _, X, pool, B) :- memberchk(X-B, PoolBoxes), !.
end_box(_, _, Shapes, X, node, s(SX, SY, W, H)) :-
    memberchk(shape(X, SX, SY, W, H), Shapes), !.
end_box(Db, PoolBoxes, Shapes, X, K, B) :-      % inside a collapsed subprocess
    member(node(X, _, _, C), Db),
    end_box(Db, PoolBoxes, Shapes, C, K, B).

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
