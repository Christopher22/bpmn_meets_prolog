/*  The classic "pizza order" collaboration (see prolog/pizza.pl).

    The page renders whatever model/1 yields. Edit away: the diagram follows.
*/
:- use_module(bpmn).
:- use_module(library(format)).
:- use_module(library(lists)).

model(bpmn(pizza_collaboration, [

    pool(customer, "Pizza Customer", [
        start(hungry, "Hungry for pizza"),
        task(choose, "Select a pizza", user),
        task(order, "Order a pizza", send),
        event_based(wait),
        catch(delivered, "Pizza received", message),
        catch(late, "60 minutes", timer("PT60M")),
        task(ask, "Ask for the pizza", send),
        task(pay, "Pay the pizza", send),
        task(eat, "Eat the pizza", manual),
        end(satisfied, "Hunger satisfied"),

        hungry => choose => order => wait,
        wait => [ delivered => pay => eat => satisfied,
                  late => ask => wait ]
    ]),

    pool(vendor, "Pizza Vendor", [
        lane(clerk,    "Clerk",        [received, calm, apologized]),
        lane(chef,     "Pizza Chef",   [bake]),
        lane(delivery, "Delivery Boy", [deliver, payment, done]),

        start(received, "Order received", message),
        subprocess(bake, "Bake the pizza", [
            start(b_start),
            task(prepare, "Prepare pizza"),
            task(oven, "Bake in oven"),
            xor(crispy, "Crispy?"),
            end(b_end),
            b_start => prepare => oven => crispy,
            crispy => [ when("yes", "${crispy}"):b_end, default:oven ]
        ]),
        boundary(slow, "45 minutes", non_interrupting(timer("PT45M")), bake),
        task(calm, "Calm customer", send),
        end(apologized),
        task(deliver, "Deliver the pizza", send),
        task(payment, "Receive payment", receive),
        end(done),

        received => bake => deliver => payment => done,
        slow => calm => apologized
    ]),

    order   ~> received,
    ask     ~> "Where is my pizza?":vendor,
    calm    ~> customer,
    deliver ~> delivered,
    pay     ~> payment
])).

%  Reason about the model: directives run on every edit, and what they
%  print appears below the editor.
:- model(M),
   findall(T, bpmn_fact(M, node(T, task(_), _, _)), Tasks),
   length(Tasks, N),
   format("~d tasks: ~w~n", [N, Tasks]).
